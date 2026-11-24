# Frontend — Middlegame Trainer 2

React/Vite client for the middlegame trainer. Code against `../docs/api-contract.md`
exactly; domain terms in `../GLOSSARY.md`.

## Commands

- `yarn dev` — dev server on :5173, proxying `/api` → `http://localhost:8080`.
- `yarn build` — `tsc -b` (strict) + `vite build`. Must pass with zero errors.
- `yarn format` — prettier (no semicolons, single quotes, width 100).
- `yarn lint` — oxlint. Must stay clean.
- Yarn 1 only, never npm.

## Structure

- `src/api/` — typed client. `types.ts` mirrors the API contract; `client.ts` is the only
  place responses are unpacked (`shapeEval` normalizes the contract's mate encoding).
  `RequestError` carries status + machine code (e.g. `no_games`).
- `src/lib/tree.ts` — the domain core. Tree = nodes `{uci, san, fenAfter, parent, children}`.
  - `treeFromUciList` rebuilds a tree from the contract's DFS pre-order UCI list
    (each move attaches at the deepest ancestor where it's legal and not already present).
  - `uciListFromTree` serializes back (DFS pre-order) — what `PUT /api/openings/:id` sends.
  - `treeFromPgn` + `mergeTree` — PGN import with full variation trees; chess.js validates
    every SAN; illegal moves are dropped, not fatal.
  - `legalDests(fen)` — chessground dests map.
- `src/lib/lichess.ts` — client-side Lichess helpers reused from the predecessor: 429-retrying
  fetch, Lichess cloud-eval (with stockfish.online fallback), screenshot URL builder.
- `src/components/Board.tsx` — chessground wrapper; owns the string↔`Key` casts.
  Promotions are auto-queens (uci keeps no promotion char).
- `src/components/MoveTree.tsx` — the move tree rendered as printed book notation:
  numbered moves, variations in parentheses, indented per depth. Click views a position;
  the × deletes the move and its whole subtree.
- `src/pages/` — `Splash` (logged-out), `Openings` (list + trash), `Editor`, `Quiz`.
- `src/App.tsx` — auth gate via `GET /api/me`; routes below the masthead.
- `src/styles.css` — single hand-written stylesheet. Palette: sage paper / ink / spruce /
  flag red / brass. Type: Newsreader (voice) + IBM Plex Mono (notation, controls).

## Gotchas

- `chessground` v9's `Key`/`Dests` types are template-literal-typed; convert at the Board
  boundary only (`key()`, `asDests()` in `Board.tsx`).
- `chess.js` 1.x `move()` **throws** on illegal input — always try/catch.
- Evals are White-POV from every source; the quiz UI flips the sign for the quiz side.
- The quiz board is movable only for the position's `side`; the editor board is movable
  for both colors (repertoire includes opponent replies).
- Editor state is unsaved until explicit Save; `beforeunload` warns when dirty.
- The eval prefetch (`fetchPositionEval`) warms the shown position's eval while the user
  thinks; it's only a fallback if the backend result lacks numbers — the backend's
  `/api/quiz/:id/move` eval is authoritative.
