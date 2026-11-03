//! packages/tv/test/player/network.test.ts, ported: where Automatic starts,
//! from what the server knows of the TV's network and the speed test it asks for.

mod common;

use common::*;
use pley_core::ApiError;
use pley_core::model::{NetworkAnswer, NetworkReport, SpeedSample};
use pley_player::autoquality::Remembered;
use pley_player::network::{NetworkIo, NetworkMemory, ServerNetwork, SpeedResult, start_ceiling};

const MIB: u64 = 1024 * 1024;

fn answer(probe: bool, ceiling: Option<i64>, since: Option<i64>) -> NetworkAnswer {
    NetworkAnswer { local: false, ceiling_kbps: ceiling, source: None, since, probe }
}

struct Io {
    asked: NetworkAnswer,
    told: Result<NetworkAnswer, ApiError>,
    speed: SpeedResult,
}

impl NetworkIo for Io {
    fn ask(&self) -> Result<NetworkAnswer, ApiError> {
        Ok(self.asked.clone())
    }

    fn tell(&self, _report: &NetworkReport) -> Result<NetworkAnswer, ApiError> {
        self.told.clone()
    }

    fn speed_test(&self) -> Option<SpeedResult> {
        Some(self.speed)
    }

    fn wall(&self) -> i64 {
        5_000
    }
}

fn io(asked: NetworkAnswer, told: Result<NetworkAnswer, ApiError>) -> Io {
    Io { asked, told, speed: SpeedResult { kbps: 20_000.0, complete: false } }
}

#[test]
fn is_the_servers_ceiling_after_the_test_it_asked_for_or_the_tests_own_70_percent_when_unheard() {
    let late = ApiError::Net(pley_core::NetError::TimedOut);
    assert_eq!(
        start_ceiling(&io(answer(true, None, None), Ok(answer(false, Some(14_000), Some(4))))),
        Some(Some(Remembered { kbps: 14_000.0, at: 4 }))
    );
    assert_eq!(
        start_ceiling(&io(answer(true, None, None), Err(late))),
        Some(Some(Remembered { kbps: 14_000.0, at: 5_000 }))
    );
    assert_eq!(
        start_ceiling(&io(answer(false, Some(8_000), Some(1)), Ok(answer(false, None, None)))),
        Some(Some(Remembered { kbps: 8_000.0, at: 1 }))
    );
}

#[test]
fn reads_the_test_and_reports_it() {
    let fake = Fake::named("http://probe:7539", item(file()), |_| info(|_| ()));
    fake.set_network(Some(answer(true, None, None)));
    fake.set_speed(Some(SpeedSample { bytes: 3 * MIB, ms: 2_000, complete: false }));
    let net = ServerNetwork::new(fake.clone(), "tizen-1");
    let kbps = ((3 * MIB * 8) as f64 / 2_000.0).round();
    assert_eq!(net.start(), Some(Some(Remembered { kbps: (kbps * 0.7).round(), at: 7 })));
    assert_eq!(fake.told(), [NetworkReport::Probe { kbps, complete: false }]);
}

#[test]
fn doesnt_test_again_soon_after_a_test_whatever_came_of_it() {
    let fake = Fake::named("http://again:7539", item(file()), |_| info(|_| ()));
    fake.set_network(Some(answer(true, None, None)));
    assert_eq!(ServerNetwork::new(fake.clone(), "c").start(), None);
    assert_eq!(ServerNetwork::new(fake.clone(), "c").start(), None);
    assert_eq!(fake.speed_tests(), 1);
}

#[test]
fn leaves_an_older_server_alone_once_it_answered_404_and_tells_a_newer_one_each_ceiling() {
    let older = Fake::named("http://older:7539", item(file()), |_| info(|_| ()));
    let net = ServerNetwork::new(older.clone(), "c");
    assert_eq!(net.start(), None);
    net.report(Some(4_000.0));
    assert!(older.told().is_empty());
    let newer = Fake::named("http://newer:7539", item(file()), |_| info(|_| ()));
    newer.set_network(Some(answer(false, None, None)));
    let n = ServerNetwork::new(newer.clone(), "c");
    assert_eq!(n.start(), Some(None));
    n.report(Some(7_499.6));
    n.report(None);
    assert_eq!(newer.told(), [NetworkReport::Ceiling(Some(7_500.0)), NetworkReport::Ceiling(None)]);
}
