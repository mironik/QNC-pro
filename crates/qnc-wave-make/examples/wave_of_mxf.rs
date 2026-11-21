//! The wave of a frame-wrapped MXF with mono 24-bit sound elements (a camera original), read
//! only: `wave_of_mxf <file> <fps>`. Prints the lanes, their frames, the time taken and the
//! loudness segments. Nothing is written.

use std::sync::atomic::AtomicBool;

fn main() {
    let mut args = std::env::args().skip(1);
    let (Some(file), Some(fps)) = (args.next(), args.next()) else {
        eprintln!("wave_of_mxf <file.MXF> <fps>");
        return;
    };
    let fps: i64 = fps.parse().expect("fps is a whole number");
    let timebase = qnc_media_model::FrameTimebase::new(fps, 1).unwrap();
    let started = std::time::Instant::now();
    let mut makers: Vec<Option<qnc_wave_make::Maker>> = Vec::new();
    let mut samples = Vec::new();
    qnc_mxf_sound::read_sound_elements(std::path::Path::new(&file), &AtomicBool::new(false), &mut |number, value| {
        let slot = usize::from(number);
        while makers.len() <= slot {
            makers.push(None);
        }
        let maker = makers[slot].get_or_insert_with(|| qnc_wave_make::Maker::new(48_000, 1, timebase).unwrap());
        samples.clear();
        qnc_mxf_sound::s24le_to_f32(value, &mut samples);
        maker.push_channel(0, &samples)
    })
    .unwrap();
    let read = started.elapsed();
    for (slot, maker) in makers.into_iter().enumerate() {
        let Some(maker) = maker else { continue };
        let made = maker.finish();
        let lane = &made.lanes[0];
        let loud = &made.loudness[0];
        let loudest = lane.iter().max().copied().unwrap_or(0);
        println!("element {slot}: {} frames, loudest {loudest} per mille, {} loudness segments", lane.len(), loud.energies.len());
    }
    println!("read in {} ms", read.as_millis());
}
