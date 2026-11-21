//! Thirteen pictures of one MXF copy into a folder given: `strip_of_mxf <file.mxf> <frames> <fps> <out dir>`.
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (file, frames, fps, out) = (std::path::Path::new(&args[1]), args[2].parse::<u64>().unwrap(), args[3].parse::<f64>().unwrap(), std::path::Path::new(&args[4]));
    let cancel = std::sync::atomic::AtomicBool::new(false);
    let places = qnc_filmstrip_pictures::places(frames);
    let started = std::time::Instant::now();
    let stored = qnc_filmstrip_pictures::intra_pictures(file, &places, &cancel).unwrap();
    println!("index read {} ms, intra pictures: {}", started.elapsed().as_millis(), stored.is_some());
    let jpegs = match stored {
        Some(pictures) => qnc_filmstrip_pictures::decode_stored("h264", &pictures, &cancel).unwrap(),
        None => qnc_filmstrip_pictures::decode_seeked(file, &places.iter().map(|f| *f as f64 / fps).collect::<Vec<_>>(), &cancel).unwrap(),
    };
    println!("decoded {} pictures in {} ms", jpegs.len(), started.elapsed().as_millis());
    std::fs::create_dir_all(out).unwrap();
    for (i, j) in jpegs.iter().enumerate() {
        std::fs::write(out.join(qnc_filmstrip_pictures::file_name(i, places[i] as f64 / fps)), j).unwrap();
    }
}
