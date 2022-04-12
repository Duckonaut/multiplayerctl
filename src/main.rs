use std::env;
use std::fs::{create_dir_all, File};
use std::process::Command;
use std::{
    io::{Error, Read, Write},
    path::PathBuf,
};
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
#[structopt(
    name = "multiplayerctl",
    about = "Simplifies audio player control across multiple players via playerctl, allowing you to switch focus."
)]
enum Args {
    #[structopt(about = "Toggles play/pause for the current player.")]
    Toggle,
    #[structopt(about = "Plays the current player.")]
    Play,
    #[structopt(about = "Pauses the current player.")]
    Pause,
    #[structopt(about = "Switches the current player to the next available one.")]
    Switch,
    #[structopt(about = "Plays next track on the current player.")]
    Next,
    #[structopt(about = "Plays previous track on the current player.")]
    Previous,
}

fn main() -> Result<(), Error> {
    let cache_path = get_cache_path()?;

    match init_if_empty_player(&cache_path) {
        Ok(_) => (),
        Err(why) => println!("{}", why),
    }

    let args = Args::from_args();

    match args {
        Args::Toggle => toggle(&cache_path),
        Args::Play => play(&cache_path),
        Args::Pause => pause(&cache_path),
        Args::Switch => match switch(&cache_path) {
            Ok(()) => (),
            Err(why) => println!("Failed to switch player: {}", why),
        },
        Args::Next => next(&cache_path),
        Args::Previous => previous(&cache_path),
    }

    Ok(())
}

fn get_cache_path() -> Result<PathBuf, Error> {
    let xdg_cache = env::var_os("XDG_CACHE_HOME");

    let home = env::var("HOME").expect("No $HOME defined!");
    let cache_base = match xdg_cache {
        Some(v) => v
            .to_str()
            .expect("$XDG_CACHE_HOME is not valid unicode!")
            .into(),
        None => format!("{}/.cache", home),
    };

    let cache_path = format!("{}/multiplayerctl", cache_base);

    let cache_path = PathBuf::from(cache_path);

    create_dir_all(&cache_path)?;

    Ok(cache_path)
}

fn init_if_empty_player(cache_path: &PathBuf) -> Result<(), String> {
    let mut file_path = cache_path.to_owned();
    file_path.push("currentplayer");

    let mut current_player = String::new();

    if file_path.exists() {
        match File::open(&file_path) {
            Ok(mut f) => match f.read_to_string(&mut current_player) {
                Ok(_) => (),
                Err(why) => return Err(format!("Failed to read cache file: {}", why)),
            },
            Err(why) => return Err(format!("Cannot open cache file: {}", why)),
        }
    }

    let all_players_output = Command::new("playerctl")
        .arg("-l")
        .output()
        .expect("Failed to execute playerctl. Are you sure it is installed?")
        .stdout;

    let all_players_str = match String::from_utf8(all_players_output) {
        Ok(v) => v,
        Err(why) => return Err(format!("Failed to get player list: {}", &why)),
    };

    let mut all_player_lines = all_players_str.lines();

    let mut file = match File::create(&file_path) {
        Ok(f) => f,
        Err(why) => return Err(format!("Failed to create cache file: {}", why)),
    };

    let mut player_valid = false;

    for l in all_player_lines.clone() {
        if l == current_player {
            player_valid = true;
        }
    }

    if !player_valid {
        current_player = String::new();
    }

    if current_player.is_empty() {
        current_player = all_player_lines.nth(0).expect("No players found!").into();
    }

    file.write_all(current_player.as_bytes())
        .expect("Failed to write cache file.");

    Ok(())
}

fn get_current_player(cache_path: &PathBuf) -> String {
    let mut file_path = cache_path.to_owned();
    file_path.push("currentplayer");

    let mut current_player = String::new();

    match File::open(&file_path) {
        Ok(mut f) => match f.read_to_string(&mut current_player) {
            Ok(_) => (),
            Err(why) => panic!("Failed to read cache file: {}", why),
        },
        Err(why) => panic!("Cannot open cache file: {}", why),
    }

    current_player
}

fn toggle(cache_path: &PathBuf) {
    let current_player = get_current_player(&cache_path);

    Command::new("playerctl")
        .arg(format!("--player={}", current_player))
        .arg("play-pause")
        .output()
        .expect("Failed to execute command");
}

fn play(cache_path: &PathBuf) {
    let current_player = get_current_player(&cache_path);

    Command::new("playerctl")
        .arg(format!("--player={}", current_player))
        .arg("play")
        .output()
        .expect("Failed to execute command");
}

fn pause(cache_path: &PathBuf) {
    let current_player = get_current_player(&cache_path);

    Command::new("playerctl")
        .arg(format!("--player={}", current_player))
        .arg("pause")
        .output()
        .expect("Failed to execute command");
}

fn switch(cache_path: &PathBuf) -> Result<(), String> {
    let mut file_path = cache_path.to_owned();
    file_path.push("currentplayer");

    let mut current_player = String::new();

    if file_path.exists() {
        match File::open(&file_path) {
            Ok(mut f) => match f.read_to_string(&mut current_player) {
                Ok(_) => (),
                Err(why) => return Err(format!("Failed to read cache file: {}", why)),
            },
            Err(why) => return Err(format!("Cannot open cache file: {}", why)),
        }
    }

    let all_players_output = Command::new("playerctl")
        .arg("-l")
        .output()
        .expect("Failed to execute playerctl. Are you sure it is installed?")
        .stdout;

    let all_players_str = match String::from_utf8(all_players_output) {
        Ok(v) => v,
        Err(why) => return Err(format!("Failed to get player list: {}", &why)),
    };

    let mut all_player_lines = all_players_str.lines();

    let line_count = all_player_lines.clone().count();

    for (i, l) in all_player_lines.clone().enumerate() {
        if l == current_player {
            current_player = all_player_lines
                .nth((i + 1) % line_count)
                .expect("Cannot get indexed player.")
                .into();

            break;
        }
    }

    let mut file = match File::create(&file_path) {
        Ok(f) => f,
        Err(why) => return Err(format!("Failed to create cache file: {}", why)),
    };

    if current_player.is_empty() {
        current_player = all_player_lines.nth(0).expect("No players found!").into();
    }

    file.write_all(current_player.as_bytes())
        .expect("Failed to write cache file.");

    Ok(())
}

fn next(cache_path: &PathBuf) {
    let current_player = get_current_player(&cache_path);

    Command::new("playerctl")
        .arg(format!("--player={}", current_player))
        .arg("next")
        .output()
        .expect("Failed to execute command");
}

fn previous(cache_path: &PathBuf) {
    let current_player = get_current_player(&cache_path);

    Command::new("playerctl")
        .arg(format!("--player={}", current_player))
        .arg("previous")
        .output()
        .expect("Failed to execute command");
}
