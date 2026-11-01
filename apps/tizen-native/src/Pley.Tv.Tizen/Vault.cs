using System;
using System.Linq;
using System.Security.Cryptography;
using System.Text;
using Pley.Tv.Core.Auth;
using Tizen.Security.SecureRepository;
using Tizen.Security.SecureRepository.Crypto;

namespace Pley.Tv.Tizen;

/// <summary>
/// AES-GCM with a 256-bit key that lives in the TV's key manager, so the key never leaves it (the
/// Android app's KeystoreVault). Each value is bound to <c>aad</c>, the server it belongs to, so
/// one server's tokens can't be passed off as another's. Sealed values are the 12-byte IV, then
/// the ciphertext and its tag.
/// </summary>
sealed class KeyManagerVault : ITokenVault
{
    const string Alias = "pley_sign_in";
    const int IvBytes = 12, TagBits = 128;

    /// <summary>Why tokens couldn't be sealed: they aren't used, and the person signs in again.</summary>
    const string Unkept = "the sign-in couldn't be kept on this device: sign in again";

    public byte[] Seal(byte[] plain, string aad)
    {
        try
        {
            EnsureKey();
            var iv = new byte[IvBytes];
            using (var random = RandomNumberGenerator.Create()) random.GetBytes(iv);
            var sealedValue = Cipher(iv, aad).Encrypt(Alias, null, plain);
            return iv.Concat(sealedValue).ToArray();
        }
        catch (Exception e) when (e is not AuthException)
        {
            throw new AuthException(Unkept, e);
        }
    }

    public byte[]? Open(byte[] sealedValue, string aad)
    {
        if (sealedValue.Length <= IvBytes) return null;
        try
        {
            var iv = sealedValue.Take(IvBytes).ToArray();
            return Cipher(iv, aad).Decrypt(Alias, null, sealedValue.Skip(IvBytes).ToArray());
        }
        catch (Exception)
        {
            // Not sealed with this key and server, or the key is gone.
            return null;
        }
    }

    static Cipher Cipher(byte[] iv, string aad) =>
        new Cipher(new AesGcmCipherParameters { IV = iv, AAD = Encoding.UTF8.GetBytes(aad), TagLength = TagBits });

    /// <summary>Makes the key on first use; the key manager keeps it for this app alone.</summary>
    static void EnsureKey()
    {
        try
        {
            // The key manager answers an empty list with an error, not with no aliases.
            if (KeyManager.GetAliases().Any(a => a == Alias || a.EndsWith(" " + Alias, StringComparison.Ordinal))) return;
        }
        catch (Exception)
        {
            // No keys yet: made below.
        }
        try
        {
            KeyManager.CreateAesKey(256, Alias, new Policy());
        }
        catch (Exception) when (HasKey())
        {
            // Made already, under a name the list didn't show as expected.
        }
    }

    /// <summary>Whether the key is there: it answers when asked to seal something.</summary>
    static bool HasKey()
    {
        try
        {
            Cipher(new byte[IvBytes], "probe").Encrypt(Alias, null, new byte[1]);
            return true;
        }
        catch (Exception)
        {
            return false;
        }
    }
}
