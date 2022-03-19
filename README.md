# multiplayerctl

A simple CLI tool to control multiple players using [playerctl](https://github.com/altdesktop/playerctl).

## Dependencies

- `playerctl`

## Purpose

Best used when mapped to media keys using your preferred method. The `switch` option lets you switch the current player without moving your mouse and manually changing the status of a player to bump it in the playerctl queue (by default playerctl commands use the last-interacted-with player)

## Features

Uses subcommands to control the playback.

| Command                      | Description                                                                    |
|:----------------------------:| -------------------------------------------------------------------------------|
| **`switch`**				   | Switches the current player, according to the order provided by `playerctl -l`.|
| **`play`**                   | Plays the current player.                                                      |
| **`pause`**                  | Pauses the current player                                                      |
| **`toggle`**                 | Toggles the current player between play/pause.	                                |
| **`next`**                   | Plays the next track on the current player.                                    |
| **`previous`**               | Plays the previous track on the current player.                                |
