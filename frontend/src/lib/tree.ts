// Opening move tree: build, rebuild-from-UCI-list (per the contract's Tree serialization),
// serialize to DFS pre-order, and parse PGN (with full variation trees) into a tree.
// chess.js validates every move; illegal moves are dropped.

import {Chess} from 'chess.js'

export interface TreeNode {
  uci: string
  san: string
  fenAfter: string
  parent: TreeNode | null
  children: TreeNode[]
}

export interface Tree {
  root: TreeNode // synthetic root: the initial position
  side: Side
}

export type Side = 'white' | 'black'

const START_FEN = 'rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1'

export function makeRoot(): TreeNode {
  return {uci: '', san: '', fenAfter: START_FEN, parent: null, children: []}
}

function playUci(fen: string, uci: string): {san: string; fenAfter: string} | null {
  const chess = new Chess(fen)
  const from = uci.slice(0, 2)
  const to = uci.slice(2, 4)
  const promotion = uci.length > 4 ? uci[4] : undefined
  try {
    const move = chess.move({from, to, promotion: promotion ?? 'q'})
    return {san: move.san, fenAfter: chess.fen()}
  } catch {
    return null
  }
}

export {playUci}

/** Resulting FEN after playing `uci` from `fen` (null if illegal). */
export function fenAfterUci(fen: string, uci: string): string | null {
  return playUci(fen, uci)?.fenAfter ?? null
}

function uciOf(move: {from: string; to: string; promotion?: string | undefined}): string {
  return move.from + move.to + (move.promotion && move.promotion !== 'q' ? move.promotion : '')
}

/**
 * Rebuild the tree from a DFS pre-order UCI list, per the contract: each move is a child
 * of the deepest earlier position where it's not already present; a move that isn't a
 * legal continuation of the current node starts a new branch from the earliest legal
 * ancestor. Returns the root and the number of moves actually incorporated.
 */
export function treeFromUciList(
  uciList: string[],
  side: Side = 'white',
): {tree: Tree; used: number} {
  const root = makeRoot()
  let cur = root
  let used = 0

  const attachOrFollow = (parent: TreeNode, uci: string): TreeNode | null => {
    const existing = parent.children.find((c) => c.uci === uci)
    if (existing) return existing
    const played = playUci(parent.fenAfter, uci)
    if (!played) return null
    const node: TreeNode = {
      uci,
      san: played.san,
      fenAfter: played.fenAfter,
      parent,
      children: [],
    }
    parent.children.push(node)
    return node
  }

  for (const uci of uciList) {
    // Try the current node first (deepest), then walk up to the root.
    let placed: TreeNode | null = null
    let scan: TreeNode | null = cur
    while (scan) {
      placed = attachOrFollow(scan, uci)
      if (placed) break
      scan = scan.parent
    }
    if (!placed) continue // illegal everywhere: drop the move
    cur = placed
    used++
  }

  return {tree: {root, side}, used}
}

/** Serialize the tree as DFS pre-order UCI (the contract's format). */
export function uciListFromTree(root: TreeNode): string[] {
  const out: string[] = []
  const walk = (node: TreeNode): void => {
    for (const child of node.children) {
      out.push(child.uci)
      walk(child)
    }
  }
  walk(root)
  return out
}

// --- PGN import ---

interface PgnToken {
  kind: 'move' | 'open' | 'close'
  text: string
}

/** Tokenize movetext: SAN moves, `(` / `)` variation markers. Comments and headers stripped. */
function tokenizeMovetext(movetext: string): PgnToken[] {
  const noComments = movetext
    .replace(/\{[^}]*\}/g, ' ') // comments
    .replace(/;[^\n]*/g, ' ') // rest-of-line comments
    .replace(/\$\d+/g, ' ') // NAGs
  const noHeaders = noComments.replace(/\[[^\]]*\]\s*/g, ' ')
  const tokens: PgnToken[] = []
  for (const raw of noHeaders.split(/\s+/)) {
    if (!raw) continue
    if (raw === '(') tokens.push({kind: 'open', text: raw})
    else if (raw === ')') tokens.push({kind: 'close', text: raw})
    else {
      // Strip move numbers and results; a token may carry just "12." — skip those.
      const cleaned = raw.replace(/^\d+\.+/, '').replace(/^\d+\.\.\./, '')
      if (!cleaned) continue
      if (/^(1-0|0-1|1\/2-1\/2|\*)$/.test(cleaned)) continue
      tokens.push({kind: 'move', text: cleaned})
    }
  }
  return tokens
}

/**
 * Parse a PGN (headers + movetext with variations) into a move tree. Every SAN is
 * validated with chess.js; unparseable input returns null. Variations become branches
 * on the tree, merged where they coincide with existing moves.
 */
export function treeFromPgn(pgn: string, side: Side = 'white'): {tree: Tree; used: number} | null {
  // A move-number token may be glued to the SAN ("7...Nc6" or "7.Nf3").
  const withSplitMoves: PgnToken[] = []
  for (const tok of tokenizeMovetext(pgn)) {
    if (tok.kind !== 'move') {
      withSplitMoves.push(tok)
      continue
    }
    const m = tok.text.match(/^(\d+\.+)(.*)$/)
    if (m && m[2]) withSplitMoves.push({kind: 'move', text: m[2]})
    else if (m && !m[2]) continue
    else withSplitMoves.push(tok)
  }

  const root = makeRoot()
  const branchStack: TreeNode[] = []
  let cur = root
  let used = 0

  const playSan = (node: TreeNode, san: string): TreeNode | null => {
    const existing = node.children.find((c) => c.san === san)
    if (existing) return existing
    const chess = new Chess(node.fenAfter)
    let move
    try {
      move = chess.move(san)
    } catch {
      return null
    }
    const child: TreeNode = {
      uci: uciOf(move),
      san: move.san,
      fenAfter: chess.fen(),
      parent: node,
      children: [],
    }
    node.children.push(child)
    return child
  }

  for (const tok of withSplitMoves) {
    if (tok.kind === 'open') {
      // A variation restates the position before the previous move.
      branchStack.push(cur)
      cur = cur.parent ?? root
    } else if (tok.kind === 'close') {
      cur = branchStack.pop() ?? root
    } else {
      const placed = playSan(cur, tok.text)
      if (!placed) continue // tolerate junk lines rather than failing the import
      cur = placed
      used++
    }
  }

  if (used === 0) return null
  return {tree: {root, side}, used}
}

/**
 * Merge the subtree `src` (a node of a second tree at the same position) into `dst`,
 * reusing existing moves where they coincide. Used by PGN import to merge a full
 * variation tree into the opening.
 */
export function mergeTree(dst: TreeNode, src: TreeNode): void {
  for (const srcChild of src.children) {
    const existing = dst.children.find((c) => c.uci === srcChild.uci)
    if (existing) {
      mergeTree(existing, srcChild)
    } else {
      // Deep-copy the subtree with parents relinked.
      const copy = (node: TreeNode, parent: TreeNode): TreeNode => {
        const copyNode: TreeNode = {...node, parent, children: []}
        copyNode.children = node.children.map((c) => copy(c, copyNode))
        return copyNode
      }
      dst.children.push(copy(srcChild, dst))
    }
  }
}

/** Legal moves from a FEN, as a chessground dests map. */
export function legalDests(fen: string): Map<string, string[]> {
  const chess = new Chess(fen)
  const dests = new Map<string, string[]>()
  for (const m of chess.moves({verbose: true})) {
    const list = dests.get(m.from) ?? []
    list.push(m.to)
    dests.set(m.from, list)
  }
  return dests
}

export function turnOf(fen: string): 'w' | 'b' {
  return new Chess(fen).turn()
}

export function sanForUci(fen: string, uci: string): string | null {
  return playUci(fen, uci)?.san ?? null
}

export {START_FEN}
