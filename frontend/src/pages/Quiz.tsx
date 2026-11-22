import {useCallback, useEffect, useMemo, useState} from 'react'
import {Link, useNavigate, useParams} from 'react-router-dom'
import {Chess} from 'chess.js'
import {
  getOpening,
  getSettings,
  playQuizMove,
  saveSettings,
  shapeEval,
  skipQuiz,
  startQuiz,
} from '../api/client'
import type {
  MastersTopMove,
  Opening,
  QuizEval,
  QuizPosition,
  SearchSettings,
  TimeControl,
  User,
} from '../api/types'
import {RequestError} from '../api/types'
import {Board} from '../components/Board'
import {fetchPositionEval, screenshotUrl} from '../lib/lichess'
import {legalDests, turnOf} from '../lib/tree'

type Phase =
  | {kind: 'settings'}
  | {kind: 'position'; quizId: string; position: QuizPosition}
  | {
      kind: 'result'
      quizId: string
      position: QuizPosition
      played: string
      afterFen: string
      masters: MastersShape
      evalData: QuizEval
    }
  | {kind: 'empty'}
  | {kind: 'error'; message: string}

interface MastersShape {
  playedPercent: number
  positionTotal: number
  topMoves: MastersTopMove[]
}

const TIME_CONTROLS: TimeControl[] = ['blitz', 'rapid', 'classical']

export function Quiz({user}: {user: User}) {
  const {id} = useParams()
  const openingId = Number(id)
  const navigate = useNavigate()

  const [opening, setOpening] = useState<Opening | null>(null)
  const [phase, setPhase] = useState<Phase>({kind: 'settings'})
  const [tc, setTc] = useState<TimeControl>('blitz')
  const [minRating, setMinRating] = useState(800)
  const [maxRating, setMaxRating] = useState(1400)
  const [prefilledTc, setPrefilledTc] = useState<TimeControl>('blitz')
  const [starting, setStarting] = useState(false)
  const [busy, setBusy] = useState(false)
  const [submitError, setSubmitError] = useState<string | null>(null)
  const [prefetchedEval, setPrefetchedEval] = useState<number | null>(null)

  useEffect(() => {
    getOpening(openingId)
      .then(setOpening)
      .catch(() => setOpening(null))
  }, [openingId])

  // Load saved search settings; fall back to the user's own rating ±200.
  useEffect(() => {
    getSettings()
      .then((s) => {
        if (s) {
          setTc(s.timeControl)
          setMinRating(s.minRating)
          setMaxRating(s.maxRating)
        } else {
          prefill('blitz')
        }
      })
      .catch(() => prefill('blitz'))
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  function prefill(timeControl: TimeControl) {
    const own = user.ratings[timeControl]
    setPrefilledTc(timeControl)
    setMinRating(own ? Math.max(own - 200, 0) : 800)
    setMaxRating(own ? own + 200 : 1400)
  }

  const changeTc = (next: TimeControl) => {
    setTc(next)
    // Re-prefill only while the user hasn't customized the numbers for this control.
    if (prefilledTc === tc) prefill(next)
  }

  const start = useCallback(() => {
    setStarting(true)
    const settings: SearchSettings = {timeControl: tc, minRating, maxRating}
    saveSettings(settings)
      .catch(() => undefined) // best-effort; the quiz can still run
      .then(() => startQuiz(openingId))
      .then((q) => setPhase({kind: 'position', quizId: q.quizId, position: q.position}))
      .catch((err: unknown) => {
        if (err instanceof RequestError && err.code === 'no_games') setPhase({kind: 'empty'})
        else setPhase({kind: 'error', message: 'Could not find a position. Try again.'})
      })
      .finally(() => setStarting(false))
  }, [openingId, tc, minRating, maxRating])

  // Prefetch the shown position's eval while the user studies it; the Annotation uses
  // it as a fallback when the backend result is missing a number.
  useEffect(() => {
    if (phase.kind !== 'position') return
    let alive = true
    setPrefetchedEval(null)
    fetchPositionEval(phase.position.fen).then((v) => {
      if (alive) setPrefetchedEval(v)
    })
    return () => {
      alive = false
    }
  }, [phase])

  if (phase.kind === 'settings') {
    return (
      <div className="quiz-settings">
        <h1>Quiz setup</h1>
        <p className="opening-sub">{opening ? (opening.name ?? 'Unnamed opening') : 'Loading…'}</p>
        <p className="margin-note">
          Positions are drawn from real games between players near your rating, inside this
          opening's range. These search settings are saved for future quizzes.
        </p>
        <label className="field-label" htmlFor="tc">
          Time control
        </label>
        <select id="tc" value={tc} onChange={(e) => changeTc(e.target.value as TimeControl)}>
          {TIME_CONTROLS.map((t) => (
            <option key={t} value={t}>
              {t}
            </option>
          ))}
        </select>
        <div className="range-fields">
          <label>
            Rating from
            <input
              type="number"
              value={minRating}
              onChange={(e) => setMinRating(Number(e.target.value))}
            />
          </label>
          <label>
            to
            <input
              type="number"
              value={maxRating}
              onChange={(e) => setMaxRating(Number(e.target.value))}
            />
          </label>
        </div>
        <button type="button" onClick={start} disabled={starting}>
          {starting ? 'Finding a position…' : 'Start quiz'}
        </button>
        <p>
          <Link to="/">Back to openings</Link>
        </p>
      </div>
    )
  }

  if (phase.kind === 'empty') {
    return (
      <div className="quiz-empty">
        <h1>No games found</h1>
        <p>
          No amateur games matched this position with the current search settings. Widen the rating
          window, or try a different position.
        </p>
        <div className="editor-actions">
          <button type="button" onClick={start} disabled={starting}>
            Try another position
          </button>
          <button type="button" className="secondary" onClick={() => setPhase({kind: 'settings'})}>
            Change search settings
          </button>
        </div>
      </div>
    )
  }

  if (phase.kind === 'error') {
    return (
      <div className="quiz-empty">
        <h1>Something went wrong</h1>
        <p>{phase.message}</p>
        <button type="button" onClick={() => setPhase({kind: 'settings'})}>
          Back to quiz setup
        </button>
      </div>
    )
  }

  const position = phase.position
  const side = position.side

  const playMove = (from: string, to: string) => {
    if (phase.kind !== 'position' || busy) return
    setBusy(true)
    setSubmitError(null)
    const uci = from + to
    playQuizMove(phase.quizId, uci)
      .then((result) => {
        setPhase({
          kind: 'result',
          quizId: phase.quizId,
          position,
          played: uci,
          afterFen: fenAfterUci(position.fen, uci),
          masters: {
            playedPercent: result.masters.playedPercent,
            positionTotal: result.masters.positionTotal,
            topMoves: result.masters.topMoves,
          },
          evalData: shapeEval(result.eval),
        })
      })
      .catch(() => {
        setSubmitError('Could not submit that move. Try again.')
        setBusy(false)
      })
  }

  const next = () => {
    if (busy) return
    setBusy(true)
    setSubmitError(null)
    skipQuiz(phase.quizId)
      .then((q) => setPhase({kind: 'position', quizId: q.quizId, position: q.position}))
      .catch(() => setSubmitError('Could not load the next position.'))
      .finally(() => setBusy(false))
  }

  if (phase.kind === 'position') {
    return (
      <PositionView
        position={position}
        minRating={minRating}
        maxRating={maxRating}
        tc={tc}
        busy={busy}
        submitError={submitError}
        onMove={playMove}
        onSkip={next}
      />
    )
  }

  // --- Result phase ---
  const {played, afterFen, masters, evalData} = phase
  const lastMove: [string, string] = [played.slice(0, 2), played.slice(2, 4)]
  const shot = screenshotUrl(afterFen, side, {from: lastMove[0], to: lastMove[1]})
  const playedSan = sanOf(position.fen, played)

  return (
    <div className="quiz">
      <div className="quiz-board">
        <div className="board-plinth">
          <Board
            fen={afterFen}
            orientation={side}
            movableColor={null}
            dests={null}
            lastMove={lastMove}
          />
        </div>
        <p className="quiz-caption">
          You played {playedSan} — move {position.moveNumber}
        </p>
        {submitError && <p className="error-note">{submitError}</p>}
      </div>
      <aside className="quiz-panel">
        <Annotation evalData={evalData} masters={masters} side={side} prefetched={prefetchedEval} />
        <div className="result-actions">
          <button type="button" onClick={next} disabled={busy}>
            Next position
          </button>
          <a className="button-like secondary" href={shot} target="_blank" rel="noreferrer">
            Screenshot
          </a>
          <a
            className="button-like secondary"
            href={position.gameUrl}
            target="_blank"
            rel="noreferrer"
          >
            Game on Lichess
          </a>
          <button type="button" className="link-button" onClick={() => navigate('/')}>
            End quiz
          </button>
        </div>
      </aside>
    </div>
  )
}

function PositionView(props: {
  position: QuizPosition
  minRating: number
  maxRating: number
  tc: TimeControl
  busy: boolean
  submitError: string | null
  onMove: (from: string, to: string) => void
  onSkip: () => void
}) {
  const {position, minRating, maxRating, tc, busy, submitError, onMove, onSkip} = props
  const side = position.side
  const canMove = turnOf(position.fen) === (side === 'white' ? 'w' : 'b')
  const dests = useMemo(
    () => (canMove ? legalDests(position.fen) : new Map<string, string[]>()),
    [canMove, position.fen],
  )

  return (
    <div className="quiz">
      <div className="quiz-board">
        <div className="board-plinth">
          <Board
            fen={position.fen}
            orientation={side}
            movableColor={side}
            dests={dests}
            onMove={onMove}
          />
        </div>
        <p className="quiz-caption">
          {side === 'white' ? 'White' : 'Black'} to move — move {position.moveNumber}
        </p>
        {submitError && <p className="error-note">{submitError}</p>}
      </div>
      <aside className="quiz-panel">
        <h2>Your move</h2>
        <p className="margin-note">
          From a real game between players rated {minRating}–{maxRating} at {tc}. One move, the way
          you'd play it over the board.
        </p>
        <button type="button" className="secondary" onClick={onSkip} disabled={busy}>
          Skip this position
        </button>
      </aside>
    </div>
  )
}

function Annotation(props: {
  evalData: QuizEval
  masters: MastersShape
  side: 'white' | 'black'
  prefetched: number | null
}) {
  const {evalData, masters, side, prefetched} = props

  // Evals are White-POV; color the delta from the quiz side's point of view.
  const pov = side === 'white' ? 1 : -1
  let deltaLabel: string
  let tone: 'good' | 'bad' | 'flat'
  if (evalData.mate !== null) {
    const m = evalData.mate * pov
    deltaLabel = m > 0 ? `Mate in ${m}` : `Mated in ${-m}`
    tone = m > 0 ? 'good' : 'bad'
  } else {
    const before = evalData.before ?? prefetched
    const after = evalData.after
    if (before === null || after === null) {
      deltaLabel = 'eval unavailable'
      tone = 'flat'
    } else {
      const delta = (after - before) * pov
      tone = delta > 0.3 ? 'good' : delta < -0.3 ? 'bad' : 'flat'
      deltaLabel = `${delta >= 0 ? '+' : '−'}${Math.abs(delta).toFixed(2)}`
    }
  }

  const percent = Math.round(masters.playedPercent * 100)

  return (
    <div className="annotation">
      <p className="annotation-line">
        <strong className={`tone-${tone}`}>{deltaLabel}</strong>{' '}
        <span className="annotation-sub">engine eval after your move</span>
      </p>
      <p className="annotation-line">
        <strong>{percent}%</strong>{' '}
        <span className="annotation-sub">of master games chose it here</span>
      </p>
      {masters.topMoves.length > 0 && (
        <p className="annotation-sub top-moves">
          Masters' top moves:{' '}
          {masters.topMoves
            .slice(0, 4)
            .map((m) => `${m.san} ${Math.round(m.percent * 100)}%`)
            .join(', ')}
        </p>
      )}
      <p className="annotation-sub">eval source: {evalData.source}</p>
    </div>
  )
}

function fenAfterUci(fen: string, uci: string): string {
  const chess = new Chess(fen)
  try {
    chess.move({from: uci.slice(0, 2), to: uci.slice(2, 4)})
    return chess.fen()
  } catch {
    return fen
  }
}

function sanOf(fen: string, uci: string): string {
  const chess = new Chess(fen)
  try {
    return chess.move({from: uci.slice(0, 2), to: uci.slice(2, 4)}).san
  } catch {
    return uci
  }
}
