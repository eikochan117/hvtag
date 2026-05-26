# hvtag

CLI tool written in Rust to manage and tag a JP ASMR audio library. It automates importing folders, fetching metadata from DLsite, downloading cover art, and writing ID3 tags to MP3 files.

Each work is identified by an **RJ code** (e.g. `RJ01306319`) or **VJ code**, which is both the folder name prefix and the primary key in the database.

---

## Requirements

- **Rust** (to build from source)
- **FFmpeg** in `PATH` — required for `--convert`
- **WireGuard** — optional, required only if DLsite is geo-restricted in your region

---

## Installation

```sh
cargo build --release
```

---

## Configuration

On first run, a config file is created at:
- Windows: `%APPDATA%\hvtag\config.toml`
- Unix: `~/.hvtag/config.toml`

```toml
[vpn]
enabled = false
provider = "Wireguard"

[vpn.wireguard]
config_path = "/path/to/wg.conf"

[tagger]
tag_separator = "; "   # use "\0" for null-byte separator (foobar2000, etc.)
merge_tags = true

[import]
source_path = "/path/to/downloads"
library_path = "/path/to/library"
```

The database is stored at:
- Windows: `%LOCALAPPDATA%\hvtag\data.db3`
- Unix: `~/.hvtag/data.db3`

---

## Workflows

### Full pipeline (new works)

```sh
hvtag --full
```

Equivalent to `--import --collect --image --tag`. Does everything in one shot:
1. Scans `source_path` for RJ/VJ folders
2. Fetches metadata from DLsite (with VPN if enabled)
3. Downloads cover art to cache (with VPN), then copies to folders
4. Tags all MP3 files with ID3 metadata
5. Moves folders from `source_path` to `library_path`

### Import new works step by step

```sh
hvtag --import --collect --image --tag
```

`--import` alone only scans and moves. Combine with any subset of `--collect`, `--image`, `--tag` as needed.

### Scan existing library

```sh
hvtag --scan
```

Registers existing folders from `library_path` into the database without fetching metadata.

### Standalone operations (works already in database)

```sh
hvtag --collect          # Fetch/refresh metadata from DLsite
hvtag --image            # Download missing covers
hvtag --tag              # (Re-)tag all MP3 files
hvtag --convert          # Convert FLAC/WAV/OGG → MP3 320kbps (requires FFmpeg)
hvtag --tag --convert    # Convert then tag
```

All standalone commands respect the `.tagged` marker — already-tagged works are skipped unless `--force` is used.

### Target a single work

```sh
hvtag --collect --rjcode RJ01234567
hvtag --tag --rjcode RJ01234567
```

### Tag management

```sh
hvtag --manage-tags      # Rename or ignore DLsite genres (applies globally to all works)
hvtag --manage-circles   # Set display name preference for circles (EN / JP / custom)
```

After changing a mapping, works that need re-tagging are flagged automatically. Run `--tag` to apply.

---

## How tagging works

- Only **MP3** files are tagged. For FLAC/WAV/OGG, run `--convert` first.
- Tags written: title, album, album artist (circle), artists (CVs), genre (DLsite tags), track number.
- Cover art is expected as `folder.jpeg` in the work folder — not embedded in the MP3.
- Track numbers are parsed from Japanese filenames (brackets `【01】`, kanji `第01話`, etc.).
- Tag separator is configurable (`"; "` by default, `"\0"` for multi-value support in some players).

---

## Modules

| Module | Role |
|--------|------|
| `dlsite` | DLsite API + HTML scraper, orchestration |
| `tagger` | ID3 tagging, cover art, conversion, track parsing |
| `folders` | RJ/VJ code types, folder scanning, database registration |
| `database` | SQLite schema, queries, custom tag/circle mappings |
| `vpn` | WireGuard lifecycle management (Windows + Unix) |
| `config` | TOML config loading with defaults |
