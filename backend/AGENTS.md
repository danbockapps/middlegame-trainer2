# Middlegame Trainer 2 — Backend

Rust/Axum + SQLite backend. Serves the `/api/*` contract from `../docs/api-contract.md`
and the static frontend build (SPA fallback) from `STATIC_DIR`.

## Commands

- `source ~/.cargo/env` first (cargo is not on the default PATH).
- `cargo run` — start the server (binds `BIND`, default `0.0.0.0:8080`).
- `cargo test` — unit tests (no network; Lichess responses are mocked with static JSON).
- `cargo clippy --all-targets` — must pass with **zero warnings**.
- `cargo fmt` — apply before finishing.
- `cp .env.example .env` — OAuth needs only `LICHESS_CLIENT_ID` (any stable string; PKCE, no secret).

## Environment

| Variable | Default | Purpose |
| --- | --- | --- |
| `LICHESS_CLIENT_ID` | unset | Any stable string (e.g. the domain name). **Lichess requires no app registration** — there is no developer portal and no client secret; auth uses PKCE (see auth.rs). Missing → auth endpoints return `{error: "not_configured"}` but the server still runs. |
| `LICHESS_REDIRECT_URI` | `http://localhost:8080/api/auth/callback` | Must match the OAuth app registration. |
| `STOCKFISH_API_KEY` | unset | stockfish.online API key (eval fallback when cloud-eval misses). Works anonymously without it, within their free-tier limits. |
| `BIND` | `0.0.0.0:8080` | Listen address. |
| `DATABASE_URL` | `sqlite://data/app.db` | SQLite file (parent dir auto-created). |
| `STATIC_DIR` | unset | Directory of frontend statics served at non-`/api` routes with SPA fallback to `index.html`. |

## Architecture

- `main.rs` — router: `/api` nest + static-file fallback; tracing.
- `config.rs` / `state.rs` — env config and shared `AppState` (pool + reqwest-backed Lichess client).
- `db.rs` — sqlx (SQLite, runtime-checked queries) pool + schema, all tables.
- `auth.rs` — Lichess OAuth authorization-code flow **with PKCE**. **Lichess does not require
  (or offer) app registration — no developer portal, no client secret.** `LICHESS_CLIENT_ID`
  is just any stable string you pick; there is nothing to register anywhere. HttpOnly `sid`
  cookie sessions in SQLite; user upsert; search-settings seeding (own rating ±200 at first login).
- `openings.rs` — CRUD, soft delete/restore/permanent delete; name auto-computed at save from the masters explorer's `opening.name` unless `nameOverridden`.
- `quiz.rs` — same-color anchor picking, players-DB search with per-user settings, quiz-session state in SQLite.
- `tree.rs` — pure domain logic: DFS pre-order tree serialization (contract's "Tree serialization"), quiz-anchor selection, naming-position selection. **Unit-tested, no I/O.**
- `game.rs` — PGN tokenizer + SAN replay to find the position before the user's Nth move.
- `eval.rs` — eval delta: Lichess cloud-eval first, stockfish.online fallback; never mixed.
- `lichess.rs` — all HTTP access to Lichess (masters + players explorer, game PGN export, cloud-eval, OAuth token exchange) and stockfish.online; 429 retry once.

## Gotchas (from the predecessor project — don't relearn them)

- **The opening explorer 401s tokenless requests** from many IPs. ALL explorer and
  game-export calls are server-side here, carrying the logged-in user's OAuth token
  as Bearer. Never move them to the browser.
- **Evals are White-POV** for both cloud-eval and stockfish.online — no per-side flipping.
- **Castling UCI differs**: the explorer encodes castling as king-takes-rook (`e1h1`)
  while user moves use king-two-squares (`e1g1`). Match on SAN (canonical), not raw UCI
  (see `Lichess::masters_stats`).
- The players DB returns only a handful of games per month window; `players_games`
  walks `YYYY-MM` windows from 2019-01 to six months ago in random order.
- SQLite stores moves as a JSON array of UCI in DFS pre-order; the client rebuilds
  the tree (see contract).
