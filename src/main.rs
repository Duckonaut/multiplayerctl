use once_cell::sync::Lazy;
use std::env;
use std::fs::{create_dir_all, File};
use std::io::{BufRead, BufReader};
use std::os::fd::AsRawFd;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::{
    io::{Error, Read, Write},
    path::PathBuf,
};
use structopt::StructOpt;

static CHANGE_SIGNAL_HANDLER: Lazy<Arc<AtomicBool>> =
    Lazy::new(|| Arc::new(AtomicBool::new(false)));

#[derive(Debug, StructOpt)]
#[structopt(
    name = "multiplayerctl",
    about = "Simplifies audio player control across multiple players via playerctl, allowing you to switch focus."
)]
enum Args {
    #[structopt(about = "Lists all available players.")]
    List,
    #[structopt(about = "Toggles play/pause for the current player.")]
    Toggle,
    #[structopt(about = "Plays the current player.")]
    Play,
    #[structopt(about = "Pauses the current player.")]
    Pause,
    #[structopt(about = "Switches the current player to the next available one.")]
    Switch {
        #[structopt(short = "p", long = "player", help = "The player to switch to.")]
        player: Option<String>,
        #[structopt(
            short = "n",
            long = "next",
            help = "Whether to switch to the next player (default behaviour)."
        )]
        next: bool,
        #[structopt(
            short = "b",
            long = "back",
            help = "Whether to switch to the previous player."
        )]
        back: bool,
    },
    #[structopt(about = "Plays next track on the current player.")]
    Next,
    #[structopt(about = "Plays previous track on the current player.")]
    Previous,
    #[structopt(about = "Prints or sets the volume of the current player.")]
    Volume {
        #[structopt(name = "VALUE", help = "The volume to set the current player to.")]
        value: Option<String>,
        #[structopt(
            short = "f",
            long = "format",
            help = "The format to use when printing the volume."
        )]
        format: Option<String>,
    },
    #[structopt(about = "Prints or sets the position of the current player.")]
    Position {
        #[structopt(name = "VALUE", help = "The position to set the current player to.")]
        value: Option<String>,
        #[structopt(
            short = "f",
            long = "format",
            help = "The format to use when printing the position."
        )]
        format: Option<String>,
    },
    #[structopt(about = "Prints the status of the current player.")]
    Status {
        #[structopt(
            short = "f",
            long = "format",
            help = "The format to use when printing the status."
        )]
        format: Option<String>,
        #[structopt(
            short = "F",
            long = "follow",
            help = "Whether to follow the status of the player."
        )]
        follow: bool,
    },
    #[structopt(about = "Prints the metadata of the current player.")]
    Metadata {
        #[structopt(
            name = "KEY",
            help = "If the key is set, only the value with the key is printed."
        )]
        key: Option<String>,
        #[structopt(
            short = "f",
            long = "format",
            help = "The format to use when printing the metadata."
        )]
        format: Option<String>,
        #[structopt(
            short = "F",
            long = "follow",
            help = "Whether to follow the status of the player."
        )]
        follow: bool,
    },
    #[structopt(about = "Prints the current player.")]
    Player,
}

fn main() -> Result<(), Error> {
    let cache_path = get_cache_path()?;

    match init_if_empty_player(&cache_path) {
        Ok(_) => (),
        Err(why) => {
            println!("{}", why);
            return Ok(());
        }
    }

    let args = Args::from_args();

    signal_hook::flag::register(
        signal_hook::consts::SIGUSR1,
        Arc::clone(&CHANGE_SIGNAL_HANDLER),
    )?;

    match args {
        Args::List => list_players(),
        Args::Toggle => toggle(&cache_path),
        Args::Play => play(&cache_path),
        Args::Pause => pause(&cache_path),
        Args::Switch { player, next, back } => match switch(&cache_path, player, next, back) {
            Ok(()) => (),
            Err(why) => println!("Failed to switch player: {}", why),
        },
        Args::Next => next(&cache_path),
        Args::Previous => previous(&cache_path),
        Args::Volume { value, format } => volume(&cache_path, &value, &format),
        Args::Position { value, format } => position(&cache_path, &value, &format),
        Args::Status { format, follow } => status(&cache_path, &format, follow),
        Args::Metadata {
            key,
            format,
            follow,
        } => metadata(&cache_path, &key, &format, follow),
        Args::Player => player(&cache_path),
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
        match all_player_lines.next() {
            Some(v) => current_player = v.to_string(),
            None => return Err(String::from("No players found!")),
        }
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

fn list_players() {
    let output = Command::new("playerctl")
        .arg("-l")
        .output()
        .expect("Failed to execute playerctl. Are you sure it is installed?");

    print!("{}", String::from_utf8(output.stdout).unwrap());
    eprint!("{}", String::from_utf8(output.stderr).unwrap());
}

fn toggle(cache_path: &PathBuf) {
    let current_player = get_current_player(cache_path);

    Command::new("playerctl")
        .arg(format!("--player={}", current_player))
        .arg("play-pause")
        .output()
        .expect("Failed to execute playerctl. Are you sure it is installed?");
}

fn play(cache_path: &PathBuf) {
    let current_player = get_current_player(cache_path);

    Command::new("playerctl")
        .arg(format!("--player={}", current_player))
        .arg("play")
        .output()
        .expect("Failed to execute playerctl. Are you sure it is installed?");
}

fn pause(cache_path: &PathBuf) {
    let current_player = get_current_player(cache_path);

    Command::new("playerctl")
        .arg(format!("--player={}", current_player))
        .arg("pause")
        .output()
        .expect("Failed to execute playerctl. Are you sure it is installed?");
}

fn switch(
    cache_path: &PathBuf,
    player: Option<String>,
    _next: bool,
    previous: bool,
) -> Result<(), String> {
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

    match player {
        Some(p) => {
            for l in all_player_lines.clone() {
                if l == p {
                    current_player = p;
                    break;
                }
            }
        }
        None => {
            for (i, l) in all_player_lines.clone().enumerate() {
                if l == current_player {
                    if previous {
                        current_player = all_player_lines
                            .nth((i + line_count - 1) % line_count)
                            .expect("Cannot get indexed player.")
                            .into();
                    } else {
                        current_player = all_player_lines
                            .nth((i + 1) % line_count)
                            .expect("Cannot get indexed player.")
                            .into();
                    }
                    break;
                }
            }
        }
    }

    let mut file = match File::create(&file_path) {
        Ok(f) => f,
        Err(why) => return Err(format!("Failed to create cache file: {}", why)),
    };

    if current_player.is_empty() {
        current_player = all_player_lines.next().expect("No players found!").into();
    }

    file.write_all(current_player.as_bytes())
        .expect("Failed to write cache file.");

    // send SIGUSR1 to all running instances of multiplayerctl
    let mut pids = Vec::new();

    for entry in std::fs::read_dir("/proc").expect("Failed to read /proc") {
        let entry = entry.expect("Failed to read /proc entry");
        let path = entry.path();

        if path.is_dir() {
            let file_name = path.file_name().unwrap().to_str().unwrap();

            if file_name.parse::<u32>().is_ok() {
                let mut file = match File::open(path.join("comm")) {
                    Ok(f) => f,
                    Err(_) => continue,
                };

                let mut contents = String::new();

                match file.read_to_string(&mut contents) {
                    Ok(_) => (),
                    Err(_) => continue,
                }

                if contents.contains("multiplayerctl") {
                    pids.push(file_name.parse::<u32>().unwrap());
                }
            }
        }
    }

    for pid in pids {
        unsafe {
            libc::kill(pid as i32, libc::SIGUSR1);
        }
    }

    Ok(())
}

fn next(cache_path: &PathBuf) {
    let current_player = get_current_player(cache_path);

    let output = Command::new("playerctl")
        .arg(format!("--player={}", current_player))
        .arg("next")
        .output()
        .expect("Failed to execute playerctl. Are you sure it is installed?");

    print!("{}", String::from_utf8(output.stdout).unwrap());
    eprint!("{}", String::from_utf8(output.stderr).unwrap());
}

fn previous(cache_path: &PathBuf) {
    let current_player = get_current_player(cache_path);

    let output = Command::new("playerctl")
        .arg(format!("--player={}", current_player))
        .arg("previous")
        .output()
        .expect("Failed to execute playerctl. Are you sure it is installed?");

    print!("{}", String::from_utf8(output.stdout).unwrap());
    eprint!("{}", String::from_utf8(output.stderr).unwrap());
}

fn volume(cache_path: &PathBuf, value: &Option<String>, format: &Option<String>) {
    let current_player = get_current_player(cache_path);

    let mut args: Vec<String> = vec![format!("--player={}", current_player), "volume".to_string()];

    match value {
        Some(v) => {
            args.push(v.to_string());
        }
        None => (),
    }

    match format {
        Some(f) => {
            args.push(format!("--format={}", f));
        }
        None => (),
    }

    let output = Command::new("playerctl")
        .args(args)
        .output()
        .expect("Failed to execute playerctl. Are you sure it is installed?");

    print!("{}", String::from_utf8(output.stdout).unwrap());
    eprint!("{}", String::from_utf8(output.stderr).unwrap());
}

fn position(cache_path: &PathBuf, value: &Option<String>, format: &Option<String>) {
    let current_player = get_current_player(cache_path);

    let mut args: Vec<String> = vec![
        format!("--player={}", current_player),
        "position".to_string(),
    ];

    match value {
        Some(v) => {
            args.push(v.to_string());
        }
        None => (),
    }

    match format {
        Some(f) => {
            args.push(format!("--format={}", f));
        }
        None => (),
    }

    let output = Command::new("playerctl")
        .args(args)
        .output()
        .expect("Failed to execute playerctl. Are you sure it is installed?");

    print!("{}", String::from_utf8(output.stdout).unwrap());
    eprint!("{}", String::from_utf8(output.stderr).unwrap());
}

fn status(cache_path: &PathBuf, format: &Option<String>, follow: bool) {
    let current_player = get_current_player(cache_path);

    let mut args: Vec<String> = vec![format!("--player={}", current_player), "status".to_string()];

    match format {
        Some(f) => {
            args.push(format!("--format={}", f));
        }
        None => (),
    }

    if !follow {
        let output = Command::new("playerctl")
            .args(args)
            .output()
            .expect("Failed to execute playerctl. Are you sure it is installed?");

        print!("{}", String::from_utf8(output.stdout).unwrap());
        eprint!("{}", String::from_utf8(output.stderr).unwrap());
    } else {
        let mut child = Command::new("playerctl")
            .args(args)
            .stdout(Stdio::piped())
            .spawn()
            .expect("Failed to execute playerctl. Are you sure it is installed?");

        let stdout = child.stdout.as_mut().expect("Failed to get stdout.");

        let mut reader = BufReader::new(stdout);

        let mut line = String::new();

        loop {
            line.clear();

            reader.read_line(&mut line).expect("Failed to read line.");

            print!("{}", line);
        }
    }
}

fn metadata(cache_path: &PathBuf, key: &Option<String>, format: &Option<String>, follow: bool) {
    let current_player = get_current_player(cache_path);

    let mut args: Vec<String> = vec![
        format!("--player={}", current_player),
        "metadata".to_string(),
    ];

    match key {
        Some(k) => {
            args.push(k.to_string());
        }
        None => (),
    }

    match format {
        Some(f) => {
            args.push(format!("--format={}", f));
        }
        None => (),
    }

    if !follow {
        let output = Command::new("playerctl")
            .args(args)
            .output()
            .expect("Failed to execute playerctl. Are you sure it is installed?");

        print!("{}", String::from_utf8(output.stdout).unwrap());
        eprint!("{}", String::from_utf8(output.stderr).unwrap());
    } else {
        args.push("--follow".to_string());

        loop {
            let current_player = get_current_player(cache_path);
            args.iter_mut().for_each(|arg| {
                if arg.starts_with("--player=") {
                    *arg = format!("--player={}", current_player);
                }
            });
            let mut child = Command::new("playerctl")
                .args(args.clone())
                .stdout(Stdio::piped())
                .spawn()
                .expect("Failed to execute playerctl. Are you sure it is installed?");

            let stdout = child.stdout.as_mut().expect("Failed to get stdout.");

            let fd = stdout.as_raw_fd();
            let flags = unsafe { libc::fcntl(fd, libc::F_GETFL, 0) };
            if flags == -1 {
                panic!("Failed to get file descriptor flags.");
            }

            let flags = flags | libc::O_NONBLOCK;

            let res = unsafe { libc::fcntl(fd, libc::F_SETFL, flags) };
            if res == -1 {
                panic!("Failed to set non-blocking mode.");
            }

            let mut buf = [0u8; 1024];

            loop {
                if CHANGE_SIGNAL_HANDLER.load(Ordering::Relaxed) {
                    child.kill().expect("Failed to kill child process.");
                    CHANGE_SIGNAL_HANDLER.store(false, Ordering::Relaxed);
                    break;
                }

                let n = unsafe {
                    libc::read(
                        fd,
                        buf.as_mut_ptr() as *mut libc::c_void,
                        buf.len() as libc::size_t,
                    )
                };

                let n = if n >= 0 {
                    n as usize
                } else if n == -1 && get_errno() == libc::EAGAIN || get_errno() == libc::EWOULDBLOCK
                {
                    std::thread::sleep(std::time::Duration::from_millis(100));
                    continue;
                } else {
                    panic!("Failed to read from stdout.");
                };

                let s =
                    String::from_utf8(buf[0..n].to_vec()).expect("Failed to convert to string.");

                print!("{}", s);
            }
        }
    }
}

fn get_errno() -> i32 {
    unsafe { *libc::__errno_location() }
}

fn player(cache_path: &PathBuf) {
    let current_player = get_current_player(cache_path);

    print!("{}", current_player);
}
