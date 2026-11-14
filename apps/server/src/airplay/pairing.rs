//! Pairing with an AirPlay device, as HomeKit pairs: pair-setup proves
//! the PIN the TV shows (SRP-6a) and swaps Ed25519 public keys, once;
//! pair-verify then proves both sides hold their keys and agrees on a
//! secret for each connection (X25519), from which the connection's keys
//! come. A TV open to everyone on the network takes a transient pairing
//! instead: the fixed PIN 3939, nothing kept.
//!
//! Pley's own key pair and pairing id are derived from the server's secret
//! key (`secret.key`), so nothing private is stored with a pairing: the
//! database keeps the device's id and public key alone.

use std::io;
use std::path::Path;
use std::time::Duration;

use ring::rand::{SecureRandom, SystemRandom};
use ring::signature::{self, Ed25519KeyPair, KeyPair};
use ring::{agreement, hmac};
use tokio::io::{AsyncRead, AsyncWrite};

use super::connection::Channel;
use super::crypto::{self, Srp};
use super::tlv;

/// The longest device pairing id kept (Apple's are a MAC address or a UUID).
const MAX_ID: usize = 64;
/// The longest wait a device may ask for before another try.
const MAX_WAIT: Duration = Duration::from_secs(60 * 60);

/// Pley's identity towards AirPlay devices.
pub struct Identity {
    seed: [u8; 32],
    /// The pairing id devices know Pley by: a UUID, as a plain name fails.
    pub id: String,
}

impl Identity {
    pub fn from_secret(secret: &[u8]) -> Identity {
        let key = crate::two_step::derived_key(secret, b"pley airplay identity");
        let seed = hmac::sign(&key, b"ed25519 seed");
        let id = hmac::sign(&key, b"pairing id");
        let mut id_bytes = [0u8; 16];
        id_bytes.copy_from_slice(&id.as_ref()[..16]);
        Identity {
            seed: seed.as_ref()[..32].try_into().expect("HMAC-SHA256 gives 32 bytes"),
            id: uuid::Builder::from_random_bytes(id_bytes).into_uuid().to_string().to_uppercase(),
        }
    }

    /// The identity of the server whose data is in `data_dir`, if its
    /// secret key can be read.
    pub fn load(data_dir: &Path) -> Option<Identity> {
        let secret = crate::two_step::load_key(&data_dir.join(crate::two_step::KEY_FILE)).ok()?;
        Some(Identity::from_secret(&secret))
    }

    fn keys(&self) -> Ed25519KeyPair {
        Ed25519KeyPair::from_seed_unchecked(&self.seed).expect("a 32-byte seed")
    }
}

/// A device Pley paired with: its pairing id and public key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Paired {
    pub id: String,
    pub public_key: Vec<u8>,
}

#[derive(Debug)]
pub enum Failed {
    /// The device wants a PIN: it isn't open to everyone on the network.
    WantsPin,
    /// The PIN was wrong.
    WrongPin,
    /// The device takes no other try for this long.
    Wait(Duration),
    /// The device no longer knows Pley: it was unpaired on the device.
    Forgotten,
    /// The device at the paired id signs with another key: it was reset,
    /// or isn't the device paired with. Only a new pairing settles it.
    OtherKey,
    /// The device refused, or answered in a way pairing doesn't go.
    Refused(&'static str),
    Io(io::Error),
}

impl From<io::Error> for Failed {
    fn from(err: io::Error) -> Failed {
        Failed::Io(err)
    }
}

impl std::fmt::Display for Failed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Failed::WantsPin => write!(f, "the device wants a PIN"),
            Failed::WrongPin => write!(f, "the PIN was wrong"),
            Failed::Wait(wait) => write!(f, "the device takes no other try for {}s", wait.as_secs()),
            Failed::Forgotten => write!(f, "the device no longer knows this server"),
            Failed::OtherKey => write!(f, "the device isn't the one paired with"),
            Failed::Refused(why) => write!(f, "the device refused: {why}"),
            Failed::Io(err) => write!(f, "{err}"),
        }
    }
}

fn random() -> [u8; 32] {
    let mut bytes = [0u8; 32];
    SystemRandom::new().fill(&mut bytes).expect("the system's random numbers");
    bytes
}

/// Posts a pairing message and reads the device's answer, or the error it
/// answers with.
async fn exchange<S: AsyncRead + AsyncWrite + Unpin>(
    channel: &mut Channel<S>,
    path: &str,
    transient: bool,
    items: &[(u8, &[u8])],
) -> Result<Vec<(u8, Vec<u8>)>, Failed> {
    let hkp = if transient { "4" } else { "3" };
    let headers = [("Content-Type", "application/octet-stream"), ("X-Apple-HKP", hkp)];
    let answer = channel.request("POST", path, false, &headers, &tlv::write(items)).await?;
    match answer.status() {
        200 => {}
        // What Apple TVs answer a transient pairing when they want a PIN.
        470 => return Err(Failed::WantsPin),
        403 => return Err(Failed::Refused("not open to this server")),
        _ => return Err(Failed::Refused("an unexpected status")),
    }
    let items = tlv::read(&answer.body);
    match tlv::get(&items, tlv::ERROR).and_then(|e| e.first().copied()) {
        None => Ok(items),
        Some(tlv::ERROR_AUTHENTICATION) => Err(Failed::WrongPin),
        Some(tlv::ERROR_BACKOFF) => {
            let seconds = tlv::get(&items, tlv::RETRY_DELAY)
                .map(|d| d.iter().take(8).rev().fold(0u64, |n, b| n << 8 | u64::from(*b)))
                .unwrap_or(30);
            Err(Failed::Wait(Duration::from_secs(seconds).min(MAX_WAIT)))
        }
        Some(_) => Err(Failed::Refused("a pairing error")),
    }
}

/// Asks the device to show a PIN, for [`pair`] to prove.
pub async fn ask_for_pin<S: AsyncRead + AsyncWrite + Unpin>(channel: &mut Channel<S>) -> Result<(), Failed> {
    let answer = channel.request("POST", "/pair-pin-start", false, &[("X-Apple-HKP", "3")], &[]).await?;
    match answer.status() {
        200 => Ok(()),
        403 => Err(Failed::Refused("not open to this server")),
        _ => Err(Failed::Refused("an unexpected status")),
    }
}

/// Pair-setup's first four messages: the PIN proved; the session key.
async fn prove<S: AsyncRead + AsyncWrite + Unpin>(
    channel: &mut Channel<S>,
    pin: &str,
    transient: bool,
) -> Result<Vec<u8>, Failed> {
    let mut m1: Vec<(u8, &[u8])> = vec![(tlv::METHOD, &[0]), (tlv::STATE, &[1])];
    if transient {
        m1.push((tlv::FLAGS, &[0x10]));
    }
    let m2 = exchange(channel, "/pair-setup", transient, &m1).await?;
    let (Some(salt), Some(theirs)) = (tlv::get(&m2, tlv::SALT), tlv::get(&m2, tlv::PUBLIC_KEY)) else {
        return Err(Failed::Refused("no salt or public key"));
    };
    let srp = Srp::new(random());
    let proof = srp.prove("Pair-Setup", pin, salt, theirs).ok_or(Failed::Refused("a public key of no use"))?;
    let m3: [(u8, &[u8]); 3] = [(tlv::STATE, &[3]), (tlv::PUBLIC_KEY, &srp.public), (tlv::PROOF, &proof.ours)];
    let m4 = exchange(channel, "/pair-setup", transient, &m3).await?;
    if tlv::get(&m4, tlv::PROOF) != Some(proof.theirs.as_slice()) {
        // The device didn't prove it knows the PIN it showed.
        return Err(Failed::WrongPin);
    }
    Ok(proof.key)
}

/// A transient pairing, with a TV open to everyone on the network: the
/// secret for this connection's keys.
pub async fn transient<S: AsyncRead + AsyncWrite + Unpin>(channel: &mut Channel<S>) -> Result<Vec<u8>, Failed> {
    prove(channel, "3939", true).await
}

/// Pairs with the device for good, with the `pin` it shows.
pub async fn pair<S: AsyncRead + AsyncWrite + Unpin>(
    channel: &mut Channel<S>,
    identity: &Identity,
    pin: &str,
) -> Result<Paired, Failed> {
    let key = prove(channel, pin, false).await?;
    let ours = identity.keys();
    let our_public = ours.public_key().as_ref();
    let x = crypto::derive(&key, "Pair-Setup-Controller-Sign-Salt", "Pair-Setup-Controller-Sign-Info");
    let signed = ours.sign(&[&x[..], identity.id.as_bytes(), our_public].concat());
    let sub = tlv::write(&[
        (tlv::IDENTIFIER, identity.id.as_bytes()),
        (tlv::PUBLIC_KEY, our_public),
        (tlv::SIGNATURE, signed.as_ref()),
    ]);
    let sealing = crypto::derive(&key, "Pair-Setup-Encrypt-Salt", "Pair-Setup-Encrypt-Info");
    let sealed = crypto::seal(&sealing, crypto::named_nonce("PS-Msg05"), &[], &sub);
    let m6 = exchange(channel, "/pair-setup", false, &[(tlv::STATE, &[5]), (tlv::ENCRYPTED, &sealed)]).await?;
    let opened = tlv::get(&m6, tlv::ENCRYPTED)
        .and_then(|sealed| crypto::open(&sealing, crypto::named_nonce("PS-Msg06"), &[], sealed))
        .ok_or(Failed::Refused("a last message that doesn't open"))?;
    let device = tlv::read(&opened);
    let (Some(id), Some(public_key), Some(signed)) =
        (tlv::get(&device, tlv::IDENTIFIER), tlv::get(&device, tlv::PUBLIC_KEY), tlv::get(&device, tlv::SIGNATURE))
    else {
        return Err(Failed::Refused("no id, key or signature"));
    };
    if id.is_empty() || id.len() > MAX_ID {
        return Err(Failed::Refused("an id of no use"));
    }
    let x = crypto::derive(&key, "Pair-Setup-Accessory-Sign-Salt", "Pair-Setup-Accessory-Sign-Info");
    signature::UnparsedPublicKey::new(&signature::ED25519, public_key)
        .verify(&[&x[..], id, public_key].concat(), signed)
        .map_err(|_| Failed::Refused("a signature that doesn't verify"))?;
    Ok(Paired { id: String::from_utf8_lossy(id).into_owned(), public_key: public_key.to_vec() })
}

/// Proves Pley and the device hold the keys they paired with: the secret
/// for this connection's keys.
pub async fn verify<S: AsyncRead + AsyncWrite + Unpin>(
    channel: &mut Channel<S>,
    identity: &Identity,
    device_key: &[u8],
) -> Result<Vec<u8>, Failed> {
    let rng = SystemRandom::new();
    let ephemeral = agreement::EphemeralPrivateKey::generate(&agreement::X25519, &rng)
        .map_err(|_| Failed::Refused("no key to agree with"))?;
    let our_public = ephemeral.compute_public_key().map_err(|_| Failed::Refused("no key to agree with"))?;
    let our_public = our_public.as_ref().to_vec();
    // An error here proves nothing: the device hasn't signed anything yet,
    // nor been told who Pley is.
    let m2 = exchange(channel, "/pair-verify", false, &[(tlv::STATE, &[1]), (tlv::PUBLIC_KEY, &our_public)])
        .await
        .map_err(|failed| match failed {
            Failed::WrongPin => Failed::Refused("pair-verify refused"),
            other => other,
        })?;
    let (Some(their_public), Some(sealed)) = (tlv::get(&m2, tlv::PUBLIC_KEY), tlv::get(&m2, tlv::ENCRYPTED)) else {
        return Err(Failed::Refused("no public key"));
    };
    let shared = agreement::agree_ephemeral(
        ephemeral,
        &agreement::UnparsedPublicKey::new(&agreement::X25519, their_public),
        |s| s.to_vec(),
    )
    .map_err(|_| Failed::Refused("a public key of no use"))?;
    let sealing = crypto::derive(&shared, "Pair-Verify-Encrypt-Salt", "Pair-Verify-Encrypt-Info");
    let device = crypto::open(&sealing, crypto::named_nonce("PV-Msg02"), &[], sealed)
        .map(|opened| tlv::read(&opened))
        .ok_or(Failed::Refused("a message that doesn't open"))?;
    let (Some(id), Some(signed)) = (tlv::get(&device, tlv::IDENTIFIER), tlv::get(&device, tlv::SIGNATURE)) else {
        return Err(Failed::Refused("no id or signature"));
    };
    signature::UnparsedPublicKey::new(&signature::ED25519, device_key)
        .verify(&[their_public, id, &our_public].concat(), signed)
        .map_err(|_| Failed::OtherKey)?;
    let signed = identity.keys().sign(&[&our_public[..], identity.id.as_bytes(), their_public].concat());
    let sub = tlv::write(&[(tlv::IDENTIFIER, identity.id.as_bytes()), (tlv::SIGNATURE, signed.as_ref())]);
    let sealed = crypto::seal(&sealing, crypto::named_nonce("PV-Msg03"), &[], &sub);
    exchange(channel, "/pair-verify", false, &[(tlv::STATE, &[3]), (tlv::ENCRYPTED, &sealed)])
        .await
        .map_err(forgotten)?;
    Ok(shared)
}

/// In pair-verify, an authentication error once the device has proved who
/// it is means it doesn't know Pley's key any more.
fn forgotten(failed: Failed) -> Failed {
    match failed {
        Failed::WrongPin => Failed::Forgotten,
        other => other,
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::airplay::crypto::tests::Device as Srp6;

    /// A device that pairs as an Apple TV does, over one connection.
    pub(crate) struct FakeDevice {
        pub pin: &'static str,
        keys: Ed25519KeyPair,
        pub id: &'static str,
        /// The senders it knows: their pairing id and public key.
        pub known: Vec<(Vec<u8>, Vec<u8>)>,
        srp: Option<Srp6>,
        /// Pair-setup's session key, from M3 on.
        session_key: Option<Vec<u8>>,
        verifying: Option<(Vec<u8>, Vec<u8>, [u8; 32])>,
        /// The secret the last pair-verify agreed on.
        pub shared: Option<Vec<u8>>,
    }

    const SALT: [u8; 16] = [3; 16];

    impl FakeDevice {
        pub(crate) fn new(pin: &'static str) -> FakeDevice {
            FakeDevice {
                pin,
                keys: Ed25519KeyPair::from_seed_unchecked(&[5; 32]).unwrap(),
                id: "AA:BB:CC:DD:EE:FF",
                known: Vec::new(),
                srp: None,
                session_key: None,
                verifying: None,
                shared: None,
            }
        }

        pub(crate) fn public_key(&self) -> Vec<u8> {
            self.keys.public_key().as_ref().to_vec()
        }

        /// Its answer to a pairing message.
        pub(crate) fn answer(&mut self, path: &str, body: &[u8]) -> Vec<u8> {
            let items = tlv::read(body);
            let state = tlv::get(&items, tlv::STATE).and_then(|s| s.first().copied());
            let error = |state: u8| tlv::write(&[(tlv::STATE, &[state]), (tlv::ERROR, &[tlv::ERROR_AUTHENTICATION])]);
            match (path, state) {
                ("/pair-setup", Some(1)) => {
                    let srp = Srp6::new(self.pin, &SALT);
                    let answer = tlv::write(&[(tlv::STATE, &[2]), (tlv::SALT, &SALT), (tlv::PUBLIC_KEY, &srp.public)]);
                    self.srp = Some(srp);
                    answer
                }
                ("/pair-setup", Some(3)) => {
                    let srp = self.srp.take().unwrap();
                    let a = tlv::get(&items, tlv::PUBLIC_KEY).unwrap();
                    let proof = tlv::get(&items, tlv::PROOF).unwrap();
                    if proof != srp.expected(&SALT, a) {
                        return error(4);
                    }
                    let key = srp.key(a);
                    let theirs = crypto::sha512(&[a, proof, &key]);
                    self.session_key = Some(key);
                    tlv::write(&[(tlv::STATE, &[4]), (tlv::PROOF, &theirs)])
                }
                ("/pair-setup", Some(5)) => {
                    let key = self.session_key.take().unwrap();
                    let sealing = crypto::derive(&key, "Pair-Setup-Encrypt-Salt", "Pair-Setup-Encrypt-Info");
                    let sub = crypto::open(
                        &sealing,
                        crypto::named_nonce("PS-Msg05"),
                        &[],
                        tlv::get(&items, tlv::ENCRYPTED).unwrap(),
                    )
                    .unwrap();
                    let sub = tlv::read(&sub);
                    let (id, ltpk, signed) = (
                        tlv::get(&sub, tlv::IDENTIFIER).unwrap(),
                        tlv::get(&sub, tlv::PUBLIC_KEY).unwrap(),
                        tlv::get(&sub, tlv::SIGNATURE).unwrap(),
                    );
                    let x = crypto::derive(&key, "Pair-Setup-Controller-Sign-Salt", "Pair-Setup-Controller-Sign-Info");
                    signature::UnparsedPublicKey::new(&signature::ED25519, ltpk)
                        .verify(&[&x[..], id, ltpk].concat(), signed)
                        .unwrap();
                    self.known.push((id.to_vec(), ltpk.to_vec()));
                    let ours = self.public_key();
                    let x = crypto::derive(&key, "Pair-Setup-Accessory-Sign-Salt", "Pair-Setup-Accessory-Sign-Info");
                    let signed = self.keys.sign(&[&x[..], self.id.as_bytes(), &ours].concat());
                    let sub = tlv::write(&[
                        (tlv::IDENTIFIER, self.id.as_bytes()),
                        (tlv::PUBLIC_KEY, &ours),
                        (tlv::SIGNATURE, signed.as_ref()),
                    ]);
                    let sealed = crypto::seal(&sealing, crypto::named_nonce("PS-Msg06"), &[], &sub);
                    tlv::write(&[(tlv::STATE, &[6]), (tlv::ENCRYPTED, &sealed)])
                }
                ("/pair-verify", Some(1)) => {
                    let theirs = tlv::get(&items, tlv::PUBLIC_KEY).unwrap().to_vec();
                    let rng = SystemRandom::new();
                    let ephemeral = agreement::EphemeralPrivateKey::generate(&agreement::X25519, &rng).unwrap();
                    let ours = ephemeral.compute_public_key().unwrap().as_ref().to_vec();
                    let shared = agreement::agree_ephemeral(
                        ephemeral,
                        &agreement::UnparsedPublicKey::new(&agreement::X25519, &theirs),
                        |s| s.to_vec(),
                    )
                    .unwrap();
                    let sealing = crypto::derive(&shared, "Pair-Verify-Encrypt-Salt", "Pair-Verify-Encrypt-Info");
                    let signed = self.keys.sign(&[&ours[..], self.id.as_bytes(), &theirs].concat());
                    let sub = tlv::write(&[(tlv::IDENTIFIER, self.id.as_bytes()), (tlv::SIGNATURE, signed.as_ref())]);
                    let sealed = crypto::seal(&sealing, crypto::named_nonce("PV-Msg02"), &[], &sub);
                    self.verifying = Some((theirs, ours.clone(), sealing));
                    self.shared = Some(shared);
                    tlv::write(&[(tlv::STATE, &[2]), (tlv::PUBLIC_KEY, &ours), (tlv::ENCRYPTED, &sealed)])
                }
                ("/pair-verify", Some(3)) => {
                    let (theirs, ours, sealing) = self.verifying.take().unwrap();
                    let sub = crypto::open(
                        &sealing,
                        crypto::named_nonce("PV-Msg03"),
                        &[],
                        tlv::get(&items, tlv::ENCRYPTED).unwrap(),
                    )
                    .unwrap();
                    let sub = tlv::read(&sub);
                    let (id, signed) =
                        (tlv::get(&sub, tlv::IDENTIFIER).unwrap(), tlv::get(&sub, tlv::SIGNATURE).unwrap());
                    let Some((_, ltpk)) = self.known.iter().find(|(known, _)| known == id) else { return error(4) };
                    let checked = signature::UnparsedPublicKey::new(&signature::ED25519, ltpk)
                        .verify(&[&theirs[..], id, &ours].concat(), signed);
                    if checked.is_err() {
                        return error(4);
                    }
                    tlv::write(&[(tlv::STATE, &[4])])
                }
                _ => error(0),
            }
        }
    }

    /// Serves `device` on the other end of a pipe, for as long as the pipe.
    pub(crate) fn serve(
        mut device: FakeDevice,
    ) -> (Channel<tokio::io::DuplexStream>, tokio::task::JoinHandle<FakeDevice>) {
        let (ours, theirs) = tokio::io::duplex(64 * 1024);
        let task = tokio::spawn(async move {
            let mut channel = Channel::over(theirs);
            while let Ok(request) = channel.read().await {
                let path = request.first.split_whitespace().nth(1).unwrap_or_default().to_owned();
                if path == "/pair-pin-start" {
                    channel.reply("HTTP/1.1 200 OK", &[], &[]).await.unwrap();
                    continue;
                }
                let body = device.answer(&path, &request.body);
                channel.reply("HTTP/1.1 200 OK", &[], &body).await.unwrap();
            }
            device
        });
        (Channel::over(ours), task)
    }

    fn identity() -> Identity {
        Identity::from_secret(&[1; 32])
    }

    #[tokio::test]
    async fn the_right_pin_pairs_and_verifying_agrees_on_a_secret() {
        let (mut channel, device) = serve(FakeDevice::new("4821"));
        ask_for_pin(&mut channel).await.unwrap();
        assert!(matches!(pair(&mut channel, &identity(), "4822").await, Err(Failed::WrongPin)));
        let paired = pair(&mut channel, &identity(), "4821").await.unwrap();
        assert_eq!(paired.id, "AA:BB:CC:DD:EE:FF");
        let shared = verify(&mut channel, &identity(), &paired.public_key).await.unwrap();
        drop(channel);
        let device = device.await.unwrap();
        assert_eq!(device.shared, Some(shared));
        assert_eq!(device.known, vec![(identity().id.into_bytes(), identity().keys().public_key().as_ref().to_vec())]);
    }

    #[tokio::test]
    async fn a_device_that_forgot_the_server_or_isnt_the_one_paired_with_fails_to_verify() {
        let device = FakeDevice::new("4821");
        let key = device.public_key();
        let (mut channel, _device) = serve(device);
        assert!(matches!(verify(&mut channel, &identity(), &key).await, Err(Failed::Forgotten)));
        let other = Ed25519KeyPair::from_seed_unchecked(&[6; 32]).unwrap().public_key().as_ref().to_vec();
        assert!(matches!(verify(&mut channel, &identity(), &other).await, Err(Failed::OtherKey)));
    }

    #[test]
    fn the_identity_is_the_same_for_one_secret_and_differs_for_another() {
        let (a, b) = (identity(), Identity::from_secret(&[2; 32]));
        assert_eq!(a.id, identity().id);
        assert_eq!(a.id.len(), 36);
        assert_ne!(a.id, b.id);
        assert_ne!(a.keys().public_key().as_ref(), b.keys().public_key().as_ref());
    }
}
