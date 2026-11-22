import {useCallback, useEffect, useMemo, useRef, useState} from 'react'
import {useNavigate, useParams} from 'react-router-dom'
import {getOpening, updateOpening} from '../api/client'
import type {Opening, Side} from '../api/types'
import {Board} from '../components/Board'
import {MoveTree} from '../components/MoveTree'
import {
  legalDests,
  makeRoot,
  mergeTree,
  playUci,
  treeFromPgn,
  treeFromUciList,
  uciListFromTree,
  type TreeNode,
} from '../lib/tree'

export function Editor() {
  const {id} = useParams()
  const openingId = Number(id)
  const navigate = useNavigate()

  const [opening, setOpening] = useState<Opening | null>(null)
  const [loadError, setLoadError] = useState<string | null>(null)

  const [root, setRoot] = useState<TreeNode>(() => makeRoot())
  const [current, setCurrent] = useState<TreeNode | null>(null)
  const [dirty, setDirty] = useState(false)
  const [nameOverride, setNameOverride] = useState('')
  const [rangeMin, setRangeMin] = useState(0)
  const [rangeMax, setRangeMax] = useState(2)
  const [saving, setSaving] = useState(false)
  const [saveError, setSaveError] = useState<string | null>(null)
  const [pgnOpen, setPgnOpen] = useState(false)
  const [pgnText, setPgnText] = useState('')
  const [pgnError, setPgnError] = useState<string | null>(null)
  const fileRef = useRef<HTMLInputElement | null>(null)

  useEffect(() => {
    getOpening(openingId)
      .then((o) => {
        setOpening(o)
        const {tree, used} = treeFromUciList(o.moves, o.side)
        const moveCount = o.moves.filter((m) => m !== '-').length
        if (used !== moveCount) {
          setLoadError(
            `${moveCount - used} move(s) in the saved opening could not be replayed and were dropped from the editor.`,
          )
        }
        setRoot(tree.root)
        setCurrent(lastOf(tree.root))
        setNameOverride(o.nameOverridden ? (o.name ?? '') : '')
        setRangeMin(o.rangeMin)
        setRangeMax(o.rangeMax)
      })
      .catch(() => setLoadError('Could not load this opening.'))
  }, [openingId])

  // Warn before losing unsaved work.
  useEffect(() => {
    if (!dirty) return
    const warn = (e: BeforeUnloadEvent) => {
      e.preventDefault()
    }
    window.addEventListener('beforeunload', warn)
    return () => window.removeEventListener('beforeunload', warn)
  }, [dirty])

  const side: Side = opening?.side ?? 'white'
  const orientation = side
  const viewFen = current?.fenAfter ?? makeRoot().fenAfter
  const dests = useMemo(() => legalDests(viewFen), [viewFen])

  const onBoardMove = useCallback(
    (from: string, to: string) => {
      if (!current) return
      const uci = from + to
      const existing = current.children.find((c) => c.uci === uci)
      if (existing) {
        setCurrent(existing)
        return
      }
      const played = playUci(current.fenAfter, uci)
      if (!played) return
      const node: TreeNode = {
        uci,
        san: played.san,
        fenAfter: played.fenAfter,
        parent: current,
        children: [],
      }
      current.children.push(node)
      setRoot(structuredClone(root))
      setCurrent(node)
      setDirty(true)
    },
    [current, root],
  )

  const deleteMove = (node: TreeNode) => {
    if (!node.parent) return
    node.parent.children = node.parent.children.filter((c) => c !== node)
    setCurrent(node.parent)
    setRoot(structuredClone(root))
    setDirty(true)
  }

  const importPgn = (text: string) => {
    const parsed = treeFromPgn(text, side)
    if (!parsed) {
      setPgnError('Could not read that PGN — no legal moves found.')
      return
    }
    mergeTree(root, parsed.tree.root)
    setRoot(structuredClone(root))
    setCurrent(lastOf(root))
    setDirty(true)
    setPgnOpen(false)
    setPgnText('')
    setPgnError(null)
  }

  const readPgnFile = (file: File) => {
    file.text().then(importPgn)
  }

  const save = () => {
    if (!opening) return
    setSaving(true)
    setSaveError(null)
    updateOpening(opening.id, {
      moves: uciListFromTree(root),
      nameOverride: nameOverride.trim() === '' ? null : nameOverride.trim(),
      rangeMin,
      rangeMax,
    })
      .then((o) => {
        setOpening(o)
        setDirty(false)
        setSaving(false)
      })
      .catch(() => {
        setSaveError('Could not save. Try again in a moment.')
        setSaving(false)
      })
  }

  if (loadError && !opening) {
    return <p className="error-note">{loadError}</p>
  }
  if (!opening) return <p className="loading">Loading…</p>

  const displayName = nameOverride.trim() !== '' ? nameOverride.trim() : (opening.name ?? null)

  return (
    <div className="editor">
      <div className="page-title-row">
        <div>
          <h1 className="opening-title">{displayName ?? 'Unnamed opening'}</h1>
          <p className="opening-sub">
            {side === 'white' ? 'You play White' : 'You play Black'}
            {opening.name ? ` — the books call it ${opening.name}` : ''}
          </p>
        </div>
        <div className="editor-actions">
          <button
            type="button"
            className="secondary"
            onClick={() => navigate(`/quiz/${opening.id}`)}
          >
            Quiz this opening
          </button>
          <button type="button" onClick={save} disabled={saving || !dirty}>
            {saving ? 'Saving…' : dirty ? 'Save changes' : 'Saved'}
          </button>
        </div>
      </div>

      {loadError && <p className="error-note">{loadError}</p>}
      {saveError && <p className="error-note">{saveError}</p>}

      <div className="editor-layout">
        <div className="board-column">
          <div className="board-plinth">
            <Board
              fen={viewFen}
              orientation={orientation}
              movableColor="both"
              dests={dests}
              onMove={onBoardMove}
            />
          </div>
          {dirty && <p className="dirty-note">Unsaved changes</p>}
        </div>

        <aside className="editor-panel">
          <p className="margin-note">
            Enter only moves you already know well. This trainer drills recall — an opening built
            from lines you haven't memorized won't stick.
          </p>

          <div className="pgn-box">
            <button type="button" className="link-button" onClick={() => setPgnOpen(!pgnOpen)}>
              {pgnOpen ? 'Hide PGN import' : 'Import PGN'}
            </button>
            {pgnOpen && (
              <div className="pgn-controls">
                <textarea
                  value={pgnText}
                  onChange={(e) => setPgnText(e.target.value)}
                  rows={6}
                  placeholder="Paste PGN here — variations are merged into the tree"
                />
                <div className="pgn-buttons">
                  <button
                    type="button"
                    onClick={() => importPgn(pgnText)}
                    disabled={!pgnText.trim()}
                  >
                    Import pasted PGN
                  </button>
                  <button type="button" onClick={() => fileRef.current?.click()}>
                    Choose file…
                  </button>
                  <input
                    ref={fileRef}
                    type="file"
                    accept=".pgn,text/plain"
                    hidden
                    onChange={(e) => {
                      const f = e.target.files?.[0]
                      if (f) readPgnFile(f)
                      e.target.value = ''
                    }}
                  />
                </div>
                {pgnError && <p className="error-note">{pgnError}</p>}
              </div>
            )}
          </div>

          <MoveTree
            root={root}
            current={current}
            side={side}
            onSelect={setCurrent}
            onDelete={deleteMove}
          />

          <div className="editor-fields">
            <label>
              <span className="field-label">Opening name override</span>
              <input
                type="text"
                value={nameOverride}
                onChange={(e) => {
                  setNameOverride(e.target.value)
                  setDirty(true)
                }}
                placeholder={opening.name ?? 'Leave empty to name it automatically at save'}
              />
            </label>
            <fieldset className="range-fields">
              <legend className="field-label">
                Quiz range — how many of your own moves past the repertoire end to drill (negative
                values quiz positions inside the repertoire)
              </legend>
              <label>
                from
                <input
                  type="number"
                  value={rangeMin}
                  onChange={(e) => {
                    setRangeMin(Number(e.target.value))
                    setDirty(true)
                  }}
                />
              </label>
              <label>
                to
                <input
                  type="number"
                  value={rangeMax}
                  onChange={(e) => {
                    setRangeMax(Number(e.target.value))
                    setDirty(true)
                  }}
                />
              </label>
            </fieldset>
          </div>
        </aside>
      </div>
    </div>
  )
}

function lastOf(root: TreeNode): TreeNode | null {
  let node = root
  while (node.children.length > 0) node = node.children[0]
  return node === root ? null : node
}
