# API contract (v1, working doc)

Base URL: backend serves `/api/*`; in dev, Vite proxies `/api` to `http://localhost:8080`.
Auth: cookie session (`sid`, HttpOnly). All endpoints require auth unless noted.

## Auth

- `GET /api/auth/login` → 302 to Lichess authorization URL. Query: `?next=/` optional.
- `GET /api/auth/callback?code=...&state=...` → Lichess OAuth callback; creates user + session; 302 to `next`.
- `POST /api/auth/logout` → 204, clears session.
- `GET /api/me` → `401` or:
  ```json
  { "id": "lynx", "username": "Lynx", "ratings": {"blitz": 1800, "rapid": 1650, "classical": null} }
  ```
  (`null` when no rated games in that time control.)

## Openings

- `GET /api/openings` → array of non-deleted openings:
  ```json
  [{ "id": 1, "name": "Sicilian, Najdorf", "side": "black", "nameOverridden": false,
     "rangeMin": 0, "rangeMax": 2, "moveCount": 14 }]
  ```
- `POST /api/openings` body `{ "side": "white" }` → `{ "id": 2, "name": null, "side": "white", ... }`
- `GET /api/openings/:id` → `{ ...opening, "moves": ["e2e4", "e7e5", ...], "deleted": false }`
  (`moves` = UCI, in tree DFS pre-order; client rebuilds tree — see Tree serialization below.)
- `PUT /api/openings/:id` body `{ "moves": [...uci], "nameOverride": "My Caro" | null,
  "rangeMin": 0, "rangeMax": 2 }` → full opening object (same shape as GET list item plus `moves`).
  Server computes/refreshes `name` from Lichess masters explorer unless `nameOverridden`.
- `DELETE /api/openings/:id` → soft delete (sets `deleted_at`).
- `POST /api/openings/:id/restore` → un-soft-deletes.
- `DELETE /api/openings/:id/permanent` → hard delete (only allowed when already soft-deleted).
- `GET /api/openings?deleted=1` → only soft-deleted openings.

### Tree serialization
The opening tree is a list of UCI moves in **DFS pre-order**, with a `-` marker after each
child's subtree. Reconstruct: iterate; a `-` pops the cursor one level toward the root;
each move is a child of the deepest earlier position where it's not already present; a move
that is not a legal continuation of its parent starts a new branch from the earliest legal
ancestor (chess.js validates). The `-` markers are what make sibling branches that start
with the same move unambiguous; legacy lists without them reconstruct lossily.
Leaf/dupes handled client-side too.

## Quiz

- `POST /api/quiz/start` body `{ "openingId": 1 }` →
  ```json
  { "quizId": "uuid", "position": {
      "fen": "...", "moveNumber": 8, "side": "white",
      "gameId": "abc123", "gameUrl": "https://lichess.org/abc123#24" } }
  ```
  Server: pick random same-color leaf → query Lichess amateur DB (using user's stored search
  settings: time control + min/max rating) → pick a random game and a move number within the
  opening's Range → return the FEN before the user's move. `409` with `{ "error": "no_games" }`
  if none found.
- `POST /api/quiz/:quizId/move` body `{ "uci": "g1f3" }` →
  ```json
  { "masters": { "playedPercent": 0.231, "playedTotal": 11832, "positionTotal": 51234,
                 "topMoves": [{ "san": "Nf3", "percent": 0.231 }, ...] },
    "eval": { "before": 0.31, "after": 0.29, "source": "lichess-cloud" } }
  ```
  (evals in pawns, White POV; `mate` lines encoded as `"mate": 3` / `-2` instead of `before`/`after` numbers.
  When the eval lookup fails entirely, `eval` is `{ "before": null, "after": null, "source": "unavailable" }`
  — a failed eval never fails the move.)
- `POST /api/quiz/:quizId/skip` → same shape as `POST /api/quiz/start` response (new position, same quizId).
- Screenshot URL is client-side: `https://lichess.org/export/fen.gif?fen=<fen>&color=<white|black>&lastMove=<from><to>`.

## Search settings (per user)

- `GET /api/settings` → `{ "timeControl": "blitz" | "rapid" | "classical", "minRating": 1600, "maxRating": 2000 }`
- `PUT /api/settings` body same shape → 200 with saved settings.

## Errors
Non-2xx responses: `{ "error": "machine_code", "message": "human text" }`.
