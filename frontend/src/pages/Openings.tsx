import {useEffect, useState} from 'react'
import {Link, useNavigate} from 'react-router-dom'
import {
  createOpening,
  deleteOpening,
  deleteOpeningPermanently,
  listOpenings,
  restoreOpening,
} from '../api/client'
import type {OpeningSummary, Side} from '../api/types'

export function Openings() {
  const [openings, setOpenings] = useState<OpeningSummary[] | null>(null)
  const [deleted, setDeleted] = useState<OpeningSummary[] | null>(null)
  const [creating, setCreating] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const navigate = useNavigate()

  useEffect(() => {
    listOpenings(false)
      .then(setOpenings)
      .catch(() => setError('Could not load your openings.'))
    listOpenings(true)
      .then(setDeleted)
      .catch(() => setDeleted([]))
  }, [])

  const create = (side: Side) => {
    setCreating(true)
    createOpening(side)
      .then((o) => navigate(`/openings/${o.id}`))
      .catch(() => {
        setError('Could not create the opening.')
        setCreating(false)
      })
  }

  return (
    <div className="openings">
      <div className="page-title-row">
        <h1>Your openings</h1>
        <span className="new-opening">
          <button type="button" disabled={creating} onClick={() => create('white')}>
            New opening — White
          </button>
          <button type="button" disabled={creating} onClick={() => create('black')}>
            New opening — Black
          </button>
        </span>
      </div>
      {error && <p className="error-note">{error}</p>}
      {openings === null ? (
        <p className="loading">Loading…</p>
      ) : openings.length === 0 ? (
        <p className="empty-note">
          No openings yet. Create one, play the moves you know on the board, and save.
        </p>
      ) : (
        <ul className="opening-list">
          {openings.map((o) => (
            <li key={o.id}>
              <Link to={`/openings/${o.id}`} className="opening-row">
                <span className={`side-badge side-${o.side}`}>
                  {o.side === 'white' ? '♔' : '♚'}
                </span>
                <span className="opening-name">{o.name ?? 'Unnamed opening'}</span>
                <span className="opening-meta">
                  {o.moveCount} {o.moveCount === 1 ? 'move' : 'moves'}
                </span>
              </Link>
              <span className="row-actions">
                <Link to={`/quiz/${o.id}`} className="quiz-link">
                  Quiz
                </Link>
                <button
                  type="button"
                  className="link-button danger"
                  onClick={() => void softDelete(o.id)}
                >
                  Delete
                </button>
              </span>
            </li>
          ))}
        </ul>
      )}

      {deleted !== null && deleted.length > 0 && (
        <details className="trash">
          <summary>
            Trash — {deleted.length} {deleted.length === 1 ? 'opening' : 'openings'}
          </summary>
          <ul className="trash-list">
            {deleted.map((o) => (
              <li key={o.id}>
                <span className={`side-badge side-${o.side}`}>
                  {o.side === 'white' ? '♔' : '♚'}
                </span>
                <span className="opening-name">{o.name ?? 'Unnamed opening'}</span>
                <button type="button" className="link-button" onClick={() => void restore(o.id)}>
                  Restore
                </button>
                <button
                  type="button"
                  className="link-button danger"
                  onClick={() => void remove(o.id)}
                >
                  Delete permanently
                </button>
              </li>
            ))}
          </ul>
        </details>
      )}
    </div>
  )

  async function restore(id: number) {
    await restoreOpening(id)
    setDeleted((d) => (d ?? []).filter((o) => o.id !== id))
    listOpenings(false).then(setOpenings)
  }

  async function remove(id: number) {
    await deleteOpeningPermanently(id)
    setDeleted((d) => (d ?? []).filter((o) => o.id !== id))
  }

  async function softDelete(id: number) {
    await deleteOpening(id)
    setOpenings((os) => (os ?? []).filter((o) => o.id !== id))
    listOpenings(true).then(setDeleted)
  }
}
