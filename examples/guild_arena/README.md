# Guild Arena

PvP arena game on Soroban demonstrating **guild-based social recovery** and **multi-device play** using Cougr-Core.

## Overview

On-chain gaming has a key risk: players losing access to accounts holding progress, items, and currency. Guild Arena solves this with two Cougr-Core account patterns:

1. **Social Recovery** — guild members act as guardians who can collectively restore account access after a timelock period
2. **Multi-Device** — players register multiple device keys (desktop, mobile) with per-device permission policies

## How It Works

### Account Setup

```
Player registers → sets 3 guild members as guardians (threshold 2-of-3)
                 → adds desktop key (Full permissions)
                 → adds mobile key (PlayOnly permissions)
```

### Gameplay

Players queue for PvP matches. Combat is turn-based with three actions:

- **Attack** — standard damage
- **Defend** — reduced damage
- **Special** — high damage

Elo-style ratings update after each match. Every 3 wins triggers a level-up with stat boosts.

### Recovery Flow

```
Player loses key → Guardian 1 initiates recovery
                 → Guardian 2 approves (threshold met)
                 → 7-day timelock starts
                 → After timelock: finalize_recovery()
                 → New key active, old key revoked
                 → All stats, rating, history preserved
```

## Contract API

| Function            | Description                                     |
| ------------------- | ----------------------------------------------- |
| `register_player`   | Register with guardians and recovery config     |
| `add_device`        | Add a device key with policy (Full or PlayOnly) |
| `remove_device`     | Revoke a device key                             |
| `start_match`       | Queue for or start a PvP match                  |
| `submit_action`     | Submit combat action (Attack/Defend/Special)    |
| `initiate_recovery` | Guardian starts recovery process                |
| `approve_recovery`  | Guardian approves recovery                      |
| `finalize_recovery` | Complete recovery after timelock                |
| `get_player`        | Query player profile                            |
| `get_match`         | Query current arena state                       |

## Device Policies

| Level    | Play | Trade/Admin |
| -------- | ---- | ----------- |
| Full     | ✓    | ✓           |
| PlayOnly | ✓    | ✗           |

## Building

```bash
cargo build
stellar contract build
```

## Testing

```bash
cargo test
```

Tests cover:

- Player registration with guardians
- Multi-device management
- Device policy enforcement
- Full combat match resolution
- Rating updates after matches
- Complete recovery lifecycle (initiate → approve → timelock → finalize)
- Recovery with insufficient approvals (rejected)
- Game state preservation through recovery

## Architecture

Uses Cougr-Core ECS patterns:

**Components**: `Fighter`, `MatchRecord`, `GuildMembership`, `ArenaState`

**Systems**: Matchmaking, Combat, Rating, Recovery, Device authorization

**Storage**: Soroban persistent storage keyed by player/device addresses. Recovery and device state managed through `RecoverableAccount` and `DeviceManager` from cougr-core.

## Prerequisites

- Rust 1.89+
- `rustup target add wasm32-unknown-unknown`
- Stellar CLI (optional, for deployment)

## License

MIT OR Apache-2.0
