# olx-scraper

A small OLX search scraper that posts new listings to Discord webhooks.

This is a Rust port of [cl-olx-scraper](https://github.com/carjorvaz/cl-olx-scraper).
It keeps the same config and state format, but saves the state file atomically
and can rebuild a lost baseline without posting anything.

## Usage

```sh
CONFIG_FILE=/path/to/config STATE_DIR=/var/lib/olx-scraper olx-scraper
```

With Nix:

```sh
CONFIG_FILE=/path/to/config STATE_DIR=/var/lib/olx-scraper nix run
```

### Environment

| Variable | Required | Description |
|----------|----------|-------------|
| `CONFIG_FILE` | Yes | Config file with webhook/URL groups |
| `STATE_DIR` | No | Directory for the seen-listings state (default: current directory) |
| `SEED_ONLY` | No | If set (anything but `0`/`false`), scrape and record listings without posting |

### Config

Groups are separated by blank lines. The first line of a group is the Discord
webhook; the remaining lines are OLX search URLs.

```
https://discord.com/api/webhooks/123/abc
https://www.olx.pt/carros/?search[order]=created_at:desc
https://www.olx.pt/carros/?search[filter_float_price:to]=5000

https://discord.com/api/webhooks/456/def
https://www.olx.pt/imoveis/?search[order]=created_at:desc
```

## NixOS module

```nix
{
  inputs.olx-scraper.url = "github:carjorvaz/olx-scraper-rs";

  outputs = { self, nixpkgs, olx-scraper }: {
    nixosConfigurations.myhost = nixpkgs.lib.nixosSystem {
      modules = [
        olx-scraper.nixosModules.default
        {
          services.olx-scraper = {
            enable = true;
            configFile = "/run/agenix/olx-scraper-config";
            interval = "5min";
          };
        }
      ];
    };
  };
}
```

| Option | Default | Description |
|--------|---------|-------------|
| `enable` | `false` | Enable the scraper service |
| `configFile` | — | Config file (required) |
| `interval` | `"5min"` | How often to check |
| `stateDir` | `"/var/lib/olx-scraper"` | Directory for the state file |
| `package` | flake package | The package to run |

## Development

```sh
direnv allow      # or: nix develop
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

## State and recovery

The seen list is saved atomically: it is written to `seen-urls.txt.tmp`,
synced, and renamed over `seen-urls.txt`, and it is saved after every webhook
group. A failed write keeps the previous state file instead of replacing it
with an empty one.

If the state is ever lost, rebuild the baseline without posting:

```sh
SEED_ONLY=1 CONFIG_FILE=... STATE_DIR=... olx-scraper
```

## License

GPL-3.0-or-later. Ported from [cl-olx-scraper](https://github.com/carjorvaz/cl-olx-scraper).
