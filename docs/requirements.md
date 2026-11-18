# Requirements - Middlegame Trainer 2

This will be a web application with a Rust/Axum back end and a React/Vite front end. The purpose is for the user to build a repertoire of chess openings and then get quizzed on the middlegame positions that result from those openings. The user should authenticate using their Lichess account. We also need a splash page for users who are not logged in.

### Build UI

The user should see a list of their existing openings. There should be a button to create new, and select which side the user takes in this opening - white or black. The resulting screen should allow two ways to input opening moves: manually on a board (with notation appearing as the user makes moves - on desktop this should be on the side of the board; on mobile this is more of a design challenge), or by importing a PGN file. The user should be able to view the position by clicking on a move, and they should be able to delete a move (which will delete all moves after it). An opening is a tree of moves; it's possible for an opening to have multiple variations and lines that branch off on different moves. The app should determine the name of the opening using the Lichess API, but let the user override this. The app works best if the user only enters moves that they have memorized or know very well, so a hint or nudge to that effect may be helpful. The user should also be able to soft-delete an opening.

### Quiz UI

The user selects one of the openings they've built. They select a numeric range of number of moves for number of moves after repertoire end (more on that later). The default range should be 0 to 2. The app selects (randomly, perhaps) a same-color leaf node. For example, if the user has white in this opening, and the leaf node of the tree is a move for black, ignore that move and consider the white move preceding it to be the same-color leaf node. Use the FEN for that position to query the Lichess amateur database for games that reached that position by players near the user's blitz or rapid rating, which we have because they logged in using their Lichess account.

The app shows a position to the user. The position is from one of the Lichess games, and the move number is selected using the numeric range. For example, if the range is 1 to 3, and the leaf node is move 6, then the user is shown the position from the Lichess game before their side's move 7, 8 or 9. The numeric range should allow zero and negative numbers, which would quiz the user on moves that are in their repertoire (unless the position was arrived at via transposition, but don't worry about that).

The user plays a move and then is shown results. The results are the same pieces of info as in the predecessor project below, but the design does not need to be the same at all. % of time the move was played in the masters database, and change in engine eval. There is a next button to move to the next position, a screenshot button (which links to https://lichess.org/export/fen.gif?fen=<fen>&color=<white|black>&lastMove=<{fromSquare}{toSquare}>) and a link to the game on Lichess at the correct move number.

On the to-move screen there should also be a skip button to skip to the next position.

### Predecessor project

There is a predecessor project at ../middlegame-trainer. The new project is a complete rewrite with a different stack and design, but there are some lessons to be aware of from that attempt. Look at the files AGENTS.md and lib/api.ts in particular. There are examples of how to access the Lichess masters database, the Lichess amateur database, and engine evals, all of which we will need.

### Stack suggestions

Chessground npm package, sqlite

### UI design

Delegate all UI design and frontend implementation to a specialized subagent. That subagent must explicitly load and follow the `frontend-design` skill. The main agent should not independently perform UI design.

All design specifics above are negotiable.

### Prod

The app will be deployed to a VPS with limited system resources. Other apps on the server use Docker containers, so I have a slight preference for that, but this will be the first one that uses Rust.
