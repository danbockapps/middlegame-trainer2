# Glossary

## Opening
A user-built repertoire entry: a tree of moves from the initial position, played from a chosen
side (White or Black). The user's side's moves are the ones they "know"; opponent moves are
assumed replies. Has a name (auto-determined via Lichess at save time, overridable by the user).

## Node
A position in an Opening's move tree. The root node is the initial position. Each node except
the root is reached by one move.

## Leaf node
A Node with no children. "Same-color leaf node" is the last Node in a line whose turn it is
for the user's side in that Opening; if the deepest leaf is the opponent's turn, the
same-color leaf is the user's move preceding it.

## Quiz session
A sequence of Quiz positions drawn from one Opening, starting with the user selecting the
Opening and a Range. It continues until the user ends it. (Settled: not a fixed length.)

## Quiz position
A position from a real Lichess amateur game that passed through the same-color leaf node's
FEN, with a move number determined by the Range. The user plays one move and sees Results.

## Range
The user's numeric selection of how many of their own moves past the repertoire end (negative
or zero values mean within the repertoire) the quiz position's move number falls. Default 0–2.
Stored per Opening in the database.

## Search settings
The user-level constraints for finding Lichess amateur games: time-control type (blitz, rapid
or classical) and min/max rating. Stored per user in the database; pre-filled at registration
with the user's own rating (in the chosen time control) ±200, then user-editable.

## Results
The screen after a move is played: masters-database play-rate of the move and the engine eval
change. No repertoire-match indicator. (Settled.)

## Trash
Where soft-deleted Openings live; user can restore or permanently delete from there. (Settled.)
