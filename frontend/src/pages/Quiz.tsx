import {useCallback, useEffect, useMemo, useRef, useState} from 'react'
import {Link, useNavigate, useParams} from 'react-router-dom'
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
import {
  fenAfterUci,
  lastMoveOfSanList,
  legalDests,
  promotionOf,
  sanForUci,
  turnOf,
} from '../lib/tree'

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
  playedPercent: number | null
  playedTotal: number | null
  positionTotal: number
  topMoves: MastersTopMove[]
}

const TIME_CONTROLS: TimeControl[] = ['blitz', 'rapid', 'classical']

// Module-level constant: an identity-stable empty dests map for the non-movable board.
const EMPTY_DESTS = new Map<string, string[]>()

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
  // Bumped when a move submission fails so the board re-syncs to the quiz position
  // (chessground moves pieces optimistically; without this the board desyncs).
  const [boardReset, setBoardReset] = useState(0)

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

  // Stable inputs for the shared Board (see the layout comment below). Memoized so
  // unrelated re-renders (busy toggling, eval prefetch) never re-run Board's api.set
  // with a stale fen — that would animate the optimistically moved piece back home
  // and then forward again once the result arrives.
  const quizPos = phase.kind === 'position' || phase.kind === 'result' ? phase.position : null
  const played = phase.kind === 'result' ? phase.played : null
  const side = quizPos?.side ?? 'white'
  const canMove = !!quizPos && !played && turnOf(quizPos.fen) === (side === 'white' ? 'w' : 'b')
  const dests = useMemo(
    () => (quizPos && canMove ? legalDests(quizPos.fen) : EMPTY_DESTS),
    [quizPos, canMove],
  )
  const lastMove = useMemo(() => {
    if (!quizPos) return null
    return played
      ? ([played.slice(0, 2), played.slice(2, 4)] as [string, string])
      : lastMoveOfSanList(quizPos.moves)
  }, [quizPos, played])

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

  // From here on the phase is 'position' or 'result'; both render the SAME board
  // subtree (same element position) so React keeps the chessground instance alive
  // across the transition. Unmounting/remounting the board makes every piece jump.
  const position = phase.position
  const isResult = phase.kind === 'result'
  const afterFen = played ? (fenAfterUci(position.fen, played) ?? position.fen) : position.fen
  const playedSan = played ? (sanForUci(position.fen, played) ?? played) : null
  const moves = playedSan ? [...position.moves, playedSan] : position.moves
  const shot = played
    ? screenshotUrl(afterFen, side, {from: played.slice(0, 2), to: played.slice(2, 4)})
    : undefined

  const playMove = (from: string, to: string) => {
    if (phase.kind !== 'position' || busy) return
    setBusy(true)
    setSubmitError(null)
    // Promotions are auto-queens: append the promotion char so the backend (shakmaty)
    // and chess.js both see a well-formed 5-char UCI for promoting moves.
    const uci = from + to + (promotionOf(position.fen, from, to) ?? '')
    playQuizMove(phase.quizId, uci)
      .then((result) => {
        setPhase({
          kind: 'result',
          quizId: phase.quizId,
          position,
          played: uci,
          afterFen: fenAfterUci(position.fen, uci) ?? position.fen,
          masters: {
            playedPercent: result.masters.playedPercent,
            playedTotal: result.masters.playedTotal,
            positionTotal: result.masters.positionTotal,
            topMoves: result.masters.topMoves,
          },
          evalData: shapeEval(result.eval),
        })
      })
      .catch(() => {
        setSubmitError('Could not submit that move. Try again.')
        setBoardReset((c) => c + 1)
      })
      .finally(() => setBusy(false))
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

  // --- Position + result phases share one layout so the Board element stays in the
  // same tree position and the chessground instance survives the transition ---

  return (
    <div className="quiz">
      <div className="quiz-board">
        <div className="board-plinth">
          <Board
            fen={afterFen}
            orientation={side}
            movableColor={canMove ? side : null}
            dests={dests}
            lastMove={lastMove}
            onMove={playMove}
            resetSignal={boardReset}
          />
        </div>
        <MoveList moves={moves} />
        <p className="quiz-caption">
          {playedSan
            ? `You played ${playedSan} — move ${position.moveNumber}`
            : `${side === 'white' ? 'White' : 'Black'} to move — move ${position.moveNumber}`}
        </p>
        {submitError && <p className="error-note">{submitError}</p>}
      </div>
      <aside className="quiz-panel">
        {isResult ? (
          <>
            <Annotation
              evalData={phase.evalData}
              masters={phase.masters}
              side={side}
              prefetched={prefetchedEval}
            />
            <div className="result-actions">
              <button type="button" onClick={next} disabled={busy}>
                Next position
              </button>
              <a
                className="button-like secondary"
                href={shot ?? ''}
                target="_blank"
                rel="noreferrer"
              >
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
          </>
        ) : (
          <>
            <h2>Your move</h2>
            <p className="margin-note">
              From a real game between players rated {minRating}–{maxRating} at {tc}. One move, the
              way you'd play it over the board.
            </p>
            <button type="button" className="secondary" onClick={next} disabled={busy}>
              Skip this position
            </button>
          </>
        )}
      </aside>
    </div>
  )
}

/**
 * Horizontally-scrolling strip of the moves leading up to the quiz position,
 * rendered as printed notation. Auto-scrolls to the latest move.
 */
function MoveList({moves}: {moves: string[]}) {
  const strip = useRef<HTMLDivElement>(null)

  useEffect(() => {
    strip.current?.scrollTo({left: strip.current.scrollWidth})
  }, [moves])

  const pairs: {number: number; white: string; black: string | null}[] = []
  for (let i = 0; i < moves.length; i += 2) {
    pairs.push({number: i / 2 + 1, white: moves[i], black: moves[i + 1] ?? null})
  }

  return (
    <div className="quiz-movelist" ref={strip}>
      {pairs.map((p) => (
        <span key={p.number} className="movelist-pair">
          <span className="movelist-num">{p.number}.</span> {p.white}
          {p.black && ` ${p.black}`}
        </span>
      ))}
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
  const fmt = (n: number) => n.toFixed(2)
  let deltaLabel: string
  let evalDetail: string | null
  let tone: 'good' | 'bad' | 'flat'
  if (evalData.mate !== null) {
    const m = evalData.mate * pov
    deltaLabel = m > 0 ? `Mate in ${m}` : `Mated in ${-m}`
    tone = m > 0 ? 'good' : 'bad'
    evalDetail =
      evalData.before !== null
        ? `eval ${fmt(evalData.before)} → mate in ${evalData.mate}`
        : `eval after: mate in ${evalData.mate}`
  } else {
    const before = evalData.before ?? prefetched
    const after = evalData.after
    if (before === null || after === null) {
      deltaLabel = 'eval unavailable'
      tone = 'flat'
      evalDetail = null
    } else {
      const delta = (after - before) * pov
      tone = delta > 0.3 ? 'good' : delta < -0.3 ? 'bad' : 'flat'
      deltaLabel = `${delta >= 0 ? '+' : '−'}${Math.abs(delta).toFixed(2)}`
      evalDetail = `eval ${fmt(before)} → ${fmt(after)}`
    }
  }

  const percent = masters.playedPercent === null ? null : Math.round(masters.playedPercent * 100)
  const playedLabel =
    masters.playedTotal === null
      ? masters.positionTotal === 0
        ? 'this position never arose in the masters database'
        : 'no master game reached the played move'
      : `${masters.playedTotal.toLocaleString()} of ${masters.positionTotal.toLocaleString()} games`

  return (
    <div className="annotation">
      <div className="annotation-group">
        <h3 className="annotation-source">Engine</h3>
        <p className="annotation-line">
          <strong className={`tone-${tone}`}>{deltaLabel}</strong>
          <span className="annotation-sub">after your move{evalDetail && ` · ${evalDetail}`}</span>
        </p>
        <p className="annotation-sub annotation-footnote">source: {evalData.source}</p>
      </div>
      <div className="annotation-group annotation-group-masters">
        <h3 className="annotation-source">Masters</h3>
        <p className="annotation-line">
          <strong>{percent === null ? '—' : `${percent}%`}</strong>
          <span className="annotation-sub">
            {percent === null ? playedLabel : `of master games chose it (${playedLabel})`}
          </span>
        </p>
        {masters.topMoves.length > 0 && (
          <p className="annotation-sub top-moves">
            {masters.topMoves
              .slice(0, 4)
              .map((m) => `${m.san} ${Math.round(m.percent * 100)}%`)
              .join(', ')}
          </p>
        )}
      </div>
    </div>
  )
}
