// The move tree rendered as printed book notation: numbered moves, variations
// nested in parentheses and indented per depth. Click a move to view its position;
// the × deletes that move and everything after it.
import {type TreeNode, type Side} from '../lib/tree'

interface MoveTreeProps {
  root: TreeNode
  current: TreeNode | null
  side: Side
  onSelect: (node: TreeNode) => void
  onDelete: (node: TreeNode) => void
}

export function MoveTree({root, current, side, onSelect, onDelete}: MoveTreeProps) {
  const body = renderChildren(root, 0, 0)
  if (!body)
    return <p className="moves-empty">No moves yet — play them on the board, or paste a PGN.</p>
  return <div className="moves">{body}</div>

  function renderChildren(node: TreeNode, depth: number, startPly: number) {
    if (node.children.length === 0) return null
    const [main, ...variations] = node.children
    return (
      <>
        <span className="mainline">
          <MoveLabel node={main} ply={startPly} />
          {renderChildren(main, depth, startPly + 1)}
        </span>
        {variations.map((v, i) => (
          <div className="variation" style={{'--depth': depth + 1} as CSSProperties} key={'v' + i}>
            <span className="variation-paren" aria-hidden>
              (
            </span>
            <Variation first={v} rest={v.children} depth={depth + 1} ply={startPly} />
            <span className="variation-paren" aria-hidden>
              )
            </span>
          </div>
        ))}
      </>
    )
  }

  function Variation(props: {first: TreeNode; rest: TreeNode[]; depth: number; ply: number}) {
    const {first, rest, depth, ply} = props
    return (
      <span className="variation-body">
        <MoveLabel node={first} ply={ply} />
        {rest.map((child, i) => (
          <span key={i}>
            {child.children.length > 1 ? <br /> : ' '}
            <MoveLabel node={child} ply={ply + 1} />
            {renderChildren(child, depth, ply + 2)}
          </span>
        ))}
      </span>
    )
  }

  function MoveLabel({node, ply}: {node: TreeNode; ply: number}) {
    const moveNumber = Math.floor(ply / 2) + 1
    const isWhiteMove = ply % 2 === 0
    const prefix = isWhiteMove ? `${moveNumber}.` : `${moveNumber}…`
    const isCurrent = current === node
    const isUserMove = (isWhiteMove && side === 'white') || (!isWhiteMove && side === 'black')
    return (
      <span className={`move${isCurrent ? ' is-current' : ''}${isUserMove ? ' is-user' : ''}`}>
        <button
          type="button"
          className="move-link"
          onClick={() => onSelect(node)}
          title={`View position after ${node.san}`}
        >
          <span className="move-prefix">{prefix}</span>
          {node.san}
        </button>
        {current === node && (
          <button
            type="button"
            className="move-delete"
            onClick={(e) => {
              e.stopPropagation()
              onDelete(node)
            }}
            title={`Delete ${node.san} and everything after it`}
          >
            ×
          </button>
        )}{' '}
      </span>
    )
  }
}

type CSSProperties = import('react').CSSProperties
