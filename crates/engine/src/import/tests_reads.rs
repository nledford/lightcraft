//! Reads that stop answering (a network drive that dropped its connection): the import or scan names the file, can
//! skip it, and ✕ does not wait for it ([`super::reads`]).

use std::sync::atomic::{AtomicBool, Ordering::Relaxed};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use super::*;

/// A read that does not answer until `open`.
#[derive(Clone, Default)]
pub(crate) struct Gate(Arc<(Mutex<bool>, Condvar)>);

impl Gate {
    pub(crate) fn wait(&self) {
        let (open, bell) = &*self.0;
        let mut open = open.lock().unwrap();
        while !*open {
            open = bell.wait(open).unwrap();
        }
    }

    pub(crate) fn open(&self) {
        *self.0.0.lock().unwrap() = true;
        self.0.1.notify_all();
    }
}

/// Wait (at most 10 s) until `ok`.
pub(crate) fn until(what: &str, mut ok: impl FnMut() -> bool) {
    let started = Instant::now();
    while !ok() {
        assert!(started.elapsed() < Duration::from_secs(10), "gave up waiting for {what}");
        std::thread::sleep(Duration::from_millis(2));
    }
}

/// An import job whose reads count as stalled after 30 ms without progress, looked at every 5 ms, and whose probe is
/// `probe`.
fn job(probe: impl Fn(&str) -> Result<ProbeInfo, String> + Send + Sync + 'static) -> ImportJob {
    let mut s = Session::new();
    let mut job = ImportJob::new(&mut s, ImportOptions { mode: ImportMode::Add, ..ImportOptions::default() }).unwrap();
    job.reads.set_stall_after(Duration::from_millis(30));
    job.reads.set_poll(Duration::from_millis(5));
    job.probe = Some(Arc::new(probe));
    job
}

fn jpeg() -> Result<ProbeInfo, String> {
    Ok(ProbeInfo { format: "JPEG".into(), ..ProbeInfo::default() })
}

fn files(names: &[&str]) -> Vec<String> {
    names.iter().map(|n| std::env::temp_dir().join(format!("lc-reads-{n}")).to_string_lossy().into()).collect()
}

/// A probe that hangs on every file called `hang…`, and notes the ones it read.
fn hangs_on(gate: &Gate, read: &Arc<Mutex<Vec<String>>>) -> impl Fn(&str) -> Result<ProbeInfo, String> + Send + Sync + 'static {
    let (gate, read) = (gate.clone(), read.clone());
    move |path: &str| {
        let name = shown(path);
        if name.starts_with("lc-reads-hang") {
            gate.wait();
        }
        read.lock().unwrap().push(name);
        jpeg()
    }
}

fn failures(p: &Prepared) -> Vec<(String, String)> {
    p.items
        .iter()
        .filter_map(|i| match i {
            PreparedItem::Failed(path, why) => Some((shown(path), why.clone())),
            _ => None,
        })
        .collect()
}

fn ready(p: &Prepared) -> usize {
    p.items.iter().filter(|i| matches!(i, PreparedItem::Ready(_))).count()
}

/// Given an import whose second file never answers,
/// When ✕ is pressed,
/// Then the import ends within about a second, without waiting for that read.
#[test]
fn cancel_ends_the_import_while_a_read_hangs() {
    let gate = Gate::default();
    let mut job = job(hangs_on(&gate, &Default::default()));
    let paths = files(&["a.jpg", "hang.jpg", "b.jpg", "c.jpg"]);
    let cancel = Arc::new(AtomicBool::new(false));
    let (ended, ends) = std::sync::mpsc::channel();
    let in_job = cancel.clone();
    std::thread::spawn(move || {
        let prepared = job.prepare(&paths, &in_job);
        let _ = ended.send(prepared.len());
    });
    std::thread::sleep(Duration::from_millis(100));
    assert!(ends.try_recv().is_err(), "the hung read holds the import");
    let pressed = Instant::now();
    cancel.store(true, Relaxed);
    let r = ends.recv_timeout(Duration::from_secs(5));
    gate.open();
    assert!(r.is_ok(), "the import still waits for the read that hangs");
    assert!(pressed.elapsed() < Duration::from_millis(1500), "took {:?}", pressed.elapsed());
}

/// Given a file that has not answered for longer than the stall time,
/// When the import is asked to skip it,
/// Then that file is named while it waits, then fails with the skip reason, and the others are read and readied.
#[test]
fn a_stalled_read_is_named_and_skipping_it_fails_that_file_and_goes_on() {
    let (gate, read) = (Gate::default(), Arc::<Mutex<Vec<String>>>::default());
    let mut job = job(hangs_on(&gate, &read));
    let reads = job.reads.clone();
    let paths = files(&["a.jpg", "hang.jpg", "b.jpg", "c.jpg"]);
    assert!(reads.stalled().is_none());
    let worker = std::thread::spawn(move || job.prepare(&paths, &AtomicBool::new(false)));
    until("the stall to be reported", || reads.stalled().is_some());
    let stalled = reads.stalled().expect("stalled");
    assert_eq!(stalled.what, "lc-reads-hang.jpg");
    assert!(stalled.waited >= Duration::from_millis(30), "{:?}", stalled.waited);
    assert!(!worker.is_finished(), "the import waits for it");
    reads.skip();
    let prepared = worker.join().unwrap();
    gate.open();
    assert_eq!(prepared.len(), 4);
    assert_eq!(ready(&prepared), 3, "the others are readied");
    let failed = failures(&prepared);
    assert_eq!(failed.len(), 1, "{failed:?}");
    assert_eq!(failed[0].0, "lc-reads-hang.jpg");
    assert!(failed[0].1.starts_with("skipped: no answer after "), "{}", failed[0].1);
    assert!(failed[0].1.ends_with(" s"), "{}", failed[0].1);
    assert_eq!(reads.skipped(), 1);
    assert!(reads.stalled().is_none(), "nothing is waited for any more");
    assert!(prepared.stopped().is_none());
}

/// Given a read that is slow but keeps making progress (a big file on a slow drive, or queueing for memory),
/// Then it is never reported as waiting, however long it takes in all.
#[test]
fn a_slow_read_that_keeps_progressing_is_never_reported() {
    let seen = Arc::new(AtomicBool::new(false));
    let mut job = job(|path: &str| {
        if shown(path).starts_with("lc-reads-slow") {
            // 10 steps of 15 ms: 150 ms in all, five times the stall time, but never 30 ms without progress
            for _ in 0..10 {
                std::thread::sleep(Duration::from_millis(15));
                reads::progressed();
            }
        }
        jpeg()
    });
    let reads = job.reads.clone();
    let paths = files(&["slow.jpg", "a.jpg"]);
    let worker = std::thread::spawn(move || job.prepare(&paths, &AtomicBool::new(false)));
    while !worker.is_finished() {
        if reads.stalled().is_some() {
            seen.store(true, Relaxed);
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    let prepared = worker.join().unwrap();
    assert!(!seen.load(Relaxed), "a read that goes on was called stalled");
    assert_eq!((ready(&prepared), failures(&prepared).len()), (2, 0));
}

/// Given two reads without progress,
/// When Skip is asked for,
/// Then only the one the row names (the longer without progress) is given up; the other is named next.
#[test]
fn skip_gives_up_only_the_read_that_is_named() {
    let watch = Arc::new(ReadWatch::new(Duration::from_millis(30)));
    watch.set_poll(Duration::from_millis(5));
    let gate = Gate::default();
    let cancel = Arc::new(AtomicBool::new(false));
    let work: Arc<dyn Fn(u32) -> u32 + Send + Sync> = {
        let gate = gate.clone();
        Arc::new(move |job| {
            if job == 1 {
                // starts hanging 60 ms after job 0 did
                std::thread::sleep(Duration::from_millis(60));
                reads::progressed();
            }
            gate.wait();
            job
        })
    };
    let (w, c) = (watch.clone(), cancel.clone());
    let run = std::thread::spawn(move || {
        let pool = reads::Pool { watch: &w, cancel: &c, name: "test-read", threads: 2 };
        pool.run(vec![0u32, 1], &|i| format!("job-{i}"), work, Arc::new(|_| {}), Arc::new(|| {}), &|_| {})
    });
    until("job 0 to be named", || watch.stalled().is_some_and(|s| s.what == "job-0"));
    // (long enough for job 1 to stall as well)
    std::thread::sleep(Duration::from_millis(150));
    assert_eq!(watch.stalled().map(|s| s.what).as_deref(), Some("job-0"), "the longest is named");
    watch.skip();
    until("job 1 to be named next", || watch.skipped() == 1 && watch.stalled().is_some_and(|s| s.what == "job-1"));
    assert!(!run.is_finished(), "job 1 was not given up with it");
    watch.skip();
    let outcomes = run.join().unwrap();
    gate.open();
    assert_eq!(watch.skipped(), 2);
    assert!(outcomes.iter().all(|o| matches!(o, reads::Outcome::Failed(why) if why.starts_with("skipped: no answer after "))), "{outcomes:?}");
}

/// Given an import where file after file stops answering,
/// When each is skipped,
/// Then at the fourth skipped read the import ends with an error naming the drive's last file, instead of hanging
/// on, and a later batch is not started.
#[test]
fn the_fourth_skipped_read_ends_the_import_with_an_error() {
    let (gate, read) = (Gate::default(), Arc::<Mutex<Vec<String>>>::default());
    let mut job = job(hangs_on(&gate, &read));
    let reads = job.reads.clone();
    let mut list: Vec<String> = (0..4).map(|i| format!("hang-{i}.jpg")).collect();
    list.extend((0..8).map(|i| format!("ok-{i}.jpg")));
    let paths = files(&list.iter().map(String::as_str).collect::<Vec<_>>());
    let worker = std::thread::spawn(move || {
        let prepared = job.prepare(&paths, &AtomicBool::new(false));
        (job, prepared)
    });
    until("the import to end", || {
        if reads.stalled().is_some() {
            reads.skip();
        }
        worker.is_finished()
    });
    let (mut job, prepared) = worker.join().unwrap();
    gate.open();
    let why = prepared.stopped().expect("the import ended with an error").to_string();
    assert!(why.contains("not answering") && why.contains("lc-reads-hang-"), "{why}");
    assert_eq!(reads.ended().as_deref(), Some(why.as_str()));
    assert_eq!(reads.skipped(), MAX_SKIPPED);
    // it stays ended: the next batch is not started
    let next = job.prepare_files(files(&["later.jpg"]), &AtomicBool::new(false));
    assert!(next.is_empty() && next.stopped() == Some(why.as_str()));
    assert!(!read.lock().unwrap().iter().any(|n| n == "lc-reads-later.jpg"));
}

/// Given a read that was given up and answers later, while the run goes on,
/// Then its answer is handed to the late handler and nothing else: the file stays failed.
#[test]
fn a_late_answer_from_a_given_up_read_is_ignored_while_the_run_goes_on() {
    let watch = ReadWatch::new(Duration::from_millis(30));
    watch.set_poll(Duration::from_millis(5));
    let cancel = AtomicBool::new(false);
    let (gate, late_flag) = (Gate::default(), Gate::default());
    let late_seen = Arc::new(Mutex::new(Vec::<u32>::new()));
    // one reader: the second job only starts once the first was given up, and ends once its late answer arrived
    let pool = reads::Pool { watch: &watch, cancel: &cancel, name: "test-read", threads: 1 };
    let work: Arc<dyn Fn(u32) -> u32 + Send + Sync> = {
        let (gate, late_flag) = (gate.clone(), late_flag.clone());
        Arc::new(move |job| {
            if job == 0 {
                gate.wait();
                100
            } else {
                late_flag.wait();
                job
            }
        })
    };
    let late: Arc<dyn Fn(u32) + Send + Sync> = {
        let (late_seen, late_flag) = (late_seen.clone(), late_flag.clone());
        Arc::new(move |r| {
            late_seen.lock().unwrap().push(r);
            late_flag.open();
        })
    };
    let outcomes = std::thread::scope(|sc| {
        sc.spawn(|| {
            until("the stall to be reported", || watch.stalled().is_some());
            assert_eq!(watch.stalled().map(|s| s.what).as_deref(), Some("job-0"));
            watch.skip();
            until("the read to be given up", || watch.skipped() == 1);
            gate.open();
        });
        pool.run(vec![0u32, 1], &|i| format!("job-{i}"), work, late, Arc::new(|| {}), &|_| {})
    });
    assert!(matches!(outcomes.first(), Some(reads::Outcome::Failed(why)) if why.starts_with("skipped: no answer after ")), "{outcomes:?}");
    assert!(matches!(outcomes.get(1), Some(reads::Outcome::Done(1))), "{outcomes:?}");
    assert_eq!(*late_seen.lock().unwrap(), vec![100]);
}

/// A scan reads the files through the same pool: a probe that stops answering is named, and skipping it hands the
/// file back as a candidate that could not be read, with the skip as its reason.
#[test]
fn a_review_scan_names_a_stalled_file_and_lists_it_as_unreadable_once_skipped() {
    let dir = std::env::temp_dir().join(format!("lc-reads-scan-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    for name in ["a.jpg", "hang.jpg", "b.jpg"] {
        std::fs::write(dir.join(name), name).unwrap();
    }
    let (gate, read) = (Gate::default(), Arc::<Mutex<Vec<String>>>::default());
    let mut s = Session::new();
    let probe = hangs_on(&gate, &read);
    // (the probe's hang is keyed on the `lc-reads-` prefix the other tests give their files)
    s.media.file_probe = Some(Arc::new(move |p: &str| probe(&format!("lc-reads-{}", shown(p)))));
    let (input, paths) = ScanInput::new(&mut s, &[dir.to_string_lossy().to_string()]);
    let progress = Arc::new(ScanProgress::new(Duration::from_millis(30)));
    progress.reads.set_poll(Duration::from_millis(5));
    let p = progress.clone();
    let worker = std::thread::spawn(move || scan_with(input, &paths, &p));
    until("the stall to be reported", || progress.reads.stalled().is_some());
    assert_eq!(progress.reads.stalled().map(|s| s.what).as_deref(), Some("hang.jpg"));
    progress.reads.skip();
    let out = worker.join().unwrap();
    gate.open();
    let by_name = |n: &str| out.candidates.iter().find(|c| c.name == n).unwrap_or_else(|| panic!("no candidate {n}"));
    assert!(by_name("hang.jpg").error.as_deref().is_some_and(|e| e.starts_with("skipped: no answer after ")), "{:?}", by_name("hang.jpg"));
    assert!(by_name("a.jpg").error.is_none() && by_name("b.jpg").error.is_none());
    assert!(!out.probes.contains_key(&dir.join("hang.jpg").to_string_lossy().to_string()), "nothing is kept of a file never read");
    let _ = std::fs::remove_dir_all(&dir);
}

/// A files folder, a destination folder, and a probe that says every file is a raw.
struct Transfers {
    dir: PathBuf,
    source: PathBuf,
    dest: PathBuf,
}

impl Transfers {
    fn new(tag: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("lc-reads-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("dest")).unwrap();
        let source = dir.join("IMG_1.cr2");
        std::fs::write(&source, b"not really a raw").unwrap();
        Transfers { source, dest: dir.join("dest"), dir }
    }

    fn job(&self, mode: ImportMode, convert_dng: bool) -> ImportJob {
        let opts = ImportOptions { mode, convert_dng, destination: Some(self.dest.to_string_lossy().to_string()), ..ImportOptions::default() };
        let mut job = ImportJob::new(&mut Session::new(), opts).unwrap();
        job.reads.set_stall_after(Duration::from_millis(30));
        job.reads.set_poll(Duration::from_millis(5));
        job.probe = Some(Arc::new(|_: &str| Ok(ProbeInfo { format: "CR2".into(), kind: MediaKind::Raw, ..ProbeInfo::default() })));
        job
    }

    /// What is at the destination, as file names.
    fn left_at_destination(&self) -> Vec<String> {
        let mut found = Vec::new();
        let mut folders = vec![self.dest.clone()];
        while let Some(folder) = folders.pop() {
            for entry in std::fs::read_dir(folder).unwrap().flatten() {
                match entry.file_type() {
                    Ok(t) if t.is_dir() => folders.push(entry.path()),
                    _ => found.push(entry.file_name().to_string_lossy().to_string()),
                }
            }
        }
        found
    }

    fn assert_source_intact(&self) {
        assert_eq!(std::fs::read(&self.source).unwrap(), b"not really a raw", "the original is as it was");
    }
}

/// Given a Copy as DNG whose conversion never answers (the raw is read back from the drive for it),
/// When Skip is asked for and the drive answers later,
/// Then the file fails as skipped, the original is untouched, and the copy the late thread leaves is taken back.
#[test]
fn a_skipped_copy_as_dng_leaves_the_source_alone_and_nothing_at_the_destination() {
    let t = Transfers::new("dng");
    let gate = Gate::default();
    let mut job = t.job(ImportMode::Copy, true);
    let in_reader = gate.clone();
    job.file_bytes = Some(Arc::new(move |_: &str| {
        in_reader.wait();
        Err("the drive answered too late".into())
    }));
    let reads = job.reads.clone();
    let paths = vec![t.source.to_string_lossy().to_string()];
    let worker = std::thread::spawn(move || job.prepare(&paths, &AtomicBool::new(false)));
    until("the stall to be reported", || reads.stalled().is_some());
    reads.skip();
    let prepared = worker.join().unwrap();
    let failed = failures(&prepared);
    assert_eq!(failed.len(), 1, "{failed:?}");
    assert!(failed[0].1.starts_with("skipped: no answer after "), "{}", failed[0].1);
    assert_eq!(ready(&prepared), 0);
    t.assert_source_intact();
    gate.open();
    until("the late copy to be taken back", || t.left_at_destination().is_empty());
    t.assert_source_intact();
    let _ = std::fs::remove_dir_all(&t.dir);
}

/// Given a Move whose copy to the destination never answers half-way,
/// When Skip is asked for and the drive answers later,
/// Then the file fails as skipped, the original stays where it was, and what the late thread placed is taken back.
#[test]
fn a_skipped_move_keeps_the_source_and_leaves_nothing_at_the_destination() {
    let t = Transfers::new("move");
    crate::import_move::HANG_OPEN.store(false, std::sync::atomic::Ordering::SeqCst);
    let mut job = t.job(ImportMode::Move, false);
    let reads = job.reads.clone();
    let paths = vec![t.source.to_string_lossy().to_string()];
    let worker = std::thread::spawn(move || {
        // (the readers inherit the fault of the thread that starts them)
        crate::import_move::inject(crate::import_move::Fault::Hang);
        job.prepare(&paths, &AtomicBool::new(false))
    });
    until("the stall to be reported", || reads.stalled().is_some());
    assert!(!t.left_at_destination().is_empty(), "half a copy is there while it hangs");
    reads.skip();
    let prepared = worker.join().unwrap();
    let failed = failures(&prepared);
    assert_eq!(failed.len(), 1, "{failed:?}");
    assert!(failed[0].1.starts_with("skipped: no answer after "), "{}", failed[0].1);
    t.assert_source_intact();
    crate::import_move::HANG_OPEN.store(true, std::sync::atomic::Ordering::SeqCst);
    until("the late copy to be taken back", || t.left_at_destination().is_empty());
    t.assert_source_intact();
    let _ = std::fs::remove_dir_all(&t.dir);
}

/// Given a read that waits for memory (the gate is held by other work) for longer than the stall time,
/// Then it is never reported as stalled: waiting for memory is not a drive that stopped answering.
#[test]
fn a_read_queued_at_the_memory_gate_is_never_reported() {
    let gate = crate::memory::work_gate();
    let limit = gate.usage().1;
    // the stall time is shorter than the hold, longer than the gate's once-a-second heartbeat
    let mut job = job(|_: &str| {
        let _permit = crate::memory::work_gate().acquire(1);
        jpeg()
    });
    job.reads.set_stall_after(Duration::from_millis(1500));
    let reads = job.reads.clone();
    let held = gate.acquire(limit);
    let paths = files(&["queued.jpg"]);
    let worker = std::thread::spawn(move || job.prepare(&paths, &AtomicBool::new(false)));
    let started = Instant::now();
    while started.elapsed() < Duration::from_millis(2600) {
        assert!(reads.stalled().is_none(), "waiting at the gate was called a stall after {:?}", started.elapsed());
        std::thread::sleep(Duration::from_millis(5));
    }
    drop(held);
    let prepared = worker.join().unwrap();
    assert_eq!((ready(&prepared), failures(&prepared).len()), (1, 0));
}
