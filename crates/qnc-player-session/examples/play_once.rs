//! The live proof of the session against the real player program: opens a session with the
//! `played` JSON file given as the argument (the prepared input of a clip and its bindings),
//! waits for Ready, plays for a while, reads the pictures the player posts into the frame
//! mailbox, pauses, ends the session. Prints what happened. Reads the media; writes nothing.

use std::time::{Duration, SystemTime};

use qnc_player_session::Player;
use qnc_player_wire::{Command, Event, TransportStatus};

fn main() -> Result<(), String> {
    let file = std::env::args().nth(1).ok_or("the played JSON file")?;
    let text = std::fs::read_to_string(&file).map_err(|e| e.to_string())?;
    let played: serde_json::Map<String, serde_json::Value> = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    let mut player = Player::new();
    let started = SystemTime::now();
    let mut session = player.open(played, 1)?;
    println!("session {} open after {:?}", session.id(), started.elapsed().unwrap_or_default());

    let mut ready = false;
    for _ in 0..200 {
        for event in session.state()? {
            match event {
                Event::PlaybackReadinessChanged { ready: true, .. } => ready = true,
                Event::SourceFailed { reason, .. } | Event::PlaybackError { reason } => return Err(reason),
                _ => {}
            }
        }
        if ready {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    println!("ready after {:?}: {ready}", started.elapsed().unwrap_or_default());

    let mut mailbox = qnc_frame_map::Reader::open(session.frame_map())?;
    let play = SystemTime::now();
    for event in session.command(Command::Play)? {
        if let Event::CommandRejected { reason, .. } = event {
            return Err(reason);
        }
    }
    let mut frames = Vec::new();
    let mut pictures = 0u32;
    let mut first_picture = None;
    let mut status = TransportStatus::Empty;
    while play.elapsed().unwrap_or_default() < Duration::from_secs(3) {
        for event in session.state()? {
            if let Event::CarrierPositionChanged { frame, status: now, .. } = event {
                frames.push(frame);
                status = now;
            }
        }
        while let Some(posted) = mailbox.read_newest()? {
            if let qnc_frame_map::Posted::Picture(picture) = posted {
                pictures += 1;
                first_picture.get_or_insert((picture.header.frame, picture.header.width, picture.header.height, play.elapsed().unwrap_or_default()));
            }
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    session.command(Command::Pause)?;
    println!("status {status:?}, carrier {:?} .. {:?} over {} reports", frames.first(), frames.last(), frames.len());
    println!("pictures read from the mailbox: {pictures}; first {first_picture:?}");
    player.end(session);
    println!("ended after {:?}", started.elapsed().unwrap_or_default());
    Ok(())
}
