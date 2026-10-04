use super::*;

fn drain(hub: &mut IoHub<u8>) -> Vec<IoCall<u8>> {
    let mut calls = Vec::new();
    for _ in 0..400 {
        let (mut more, _) = hub.collect();
        let done = more
            .iter()
            .any(|call| matches!(call.args, CallArgs::Run(_)));
        calls.append(&mut more);
        if done {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    calls
}

#[test]
fn a_run_answers_once_with_its_output() {
    let mut hub = IoHub::<u8>::default();
    let mut options = SpawnOptions::new(vec!["sh".into(), "-c".into(), "echo hi".into()]);
    options.lines = false;
    let link = hub.spawn(options, IoKind::run(Some(7))).unwrap().unwrap();
    assert!(link.status().running());
    let calls = drain(&mut hub);
    let [call] = calls.as_slice() else {
        panic!("one call, got {}", calls.len());
    };
    assert_eq!(call.callback, 7);
    let CallArgs::Run(result) = &call.args else {
        panic!("a run result");
    };
    assert!(result.ok());
    assert_eq!(result.stdout, b"hi\n");
    assert!(!link.status().running());
}

#[test]
fn a_run_that_cannot_start_is_answered_later() {
    let mut hub = IoHub::<u8>::default();
    let options = SpawnOptions::new(vec!["/no/such/program".into()]);
    let error = hub
        .spawn(options, IoKind::run(Some(1)))
        .unwrap()
        .err()
        .unwrap();
    assert!(error.starts_with("/no/such/program: "));
    let (calls, more) = hub.collect();
    assert!(!more);
    let CallArgs::Run(result) = &calls[0].args else {
        panic!("a run result");
    };
    assert!(!result.ok());
    assert_eq!(result.error.as_deref(), Some(error.as_str()));
}

#[test]
fn a_closed_link_writes_nothing_and_pids_are_checked() {
    let mut hub = IoHub::<u8>::default();
    let mut options = SpawnOptions::new(vec!["sleep".into(), "5".into()]);
    options.stdin = crate::StdinMode::Pipe;
    let link = hub.spawn(options, IoKind::run(None)).unwrap().unwrap();
    assert!(link.write(&vec![0; MAX_WRITE + 1]).is_err());
    link.close();
    assert_eq!(link.write(b"x").unwrap(), Err("closed".to_string()));
    assert!(!link.kill(15));
    assert!(signalable_pid(1).is_err());
    assert!(signalable_pid(-5).is_err());
    assert_eq!(signalable_pid(1234), Ok(1234));
}
