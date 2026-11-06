//! Drives neue-audio by hand (task 3.14b). Linux only.
//!
//! ```sh
//! audio                      # print the state for two seconds
//! audio volume 0.5           # set the default sink's volume (cubic, as wpctl shows it)
//! audio mute | unmute        # the default sink
//! audio mic-mute             # toggle the default source's mute
//! audio default <node.name>  # make that sink the default
//! ```

#[cfg(target_os = "linux")]
fn main() -> anyhow::Result<()> {
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use neue_audio::{Audio, Kind, State};

    let state = Arc::new(Mutex::new(State::default()));
    let seen = Arc::clone(&state);
    let audio = Audio::start(move |s| {
        println!("{s:#?}");
        if let Ok(mut seen) = seen.lock() {
            *seen = s;
        }
    })?;
    std::thread::sleep(Duration::from_millis(500));
    let now = state
        .lock()
        .map_err(|_| anyhow::anyhow!("state lock poisoned"))?
        .clone();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let sink = now.default_endpoint(Kind::Sink);
    match (args.as_slice(), sink) {
        ([], _) => {}
        (["volume", v], Some(sink)) => audio.set_volume(sink.id, v.parse()?),
        (["mute"], Some(sink)) => audio.set_muted(sink.id, true),
        (["unmute"], Some(sink)) => audio.set_muted(sink.id, false),
        (["mic-mute"], _) => match now.default_endpoint(Kind::Source) {
            Some(source) => audio.set_muted(source.id, !source.muted),
            None => eprintln!("no default source"),
        },
        (["default", name], _) => audio.set_default(Kind::Sink, name),
        (_, None) => eprintln!("no default sink"),
        _ => eprintln!("unknown command"),
    }
    std::thread::sleep(Duration::from_millis(1500));
    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("neue-audio needs Linux");
}
