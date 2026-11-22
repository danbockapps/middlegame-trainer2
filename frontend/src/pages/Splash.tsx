import {useEffect, useRef} from 'react'
import {Chessground} from 'chessground'
import 'chessground/assets/chessground.base.css'
import 'chessground/assets/chessground.brown.css'
import 'chessground/assets/chessground.cburnett.css'

// A real middlegame position, from the Ruy Lopez closed: a quiet tension that the
// trainer exists to teach. Static diagram, never interactive.
const HERO_FEN = 'r1bqk2r/2ppbppp/p1n2n2/4p3/B3P3/5N2/PPPP1PPP/RNBQR1K1 w kq - 0 9'

export function Splash() {
  const ref = useRef<HTMLDivElement | null>(null)

  useEffect(() => {
    if (!ref.current) return
    const api = Chessground(ref.current, {
      fen: HERO_FEN,
      viewOnly: true,
      coordinates: false,
      animation: {duration: 0},
    })
    return () => api.destroy()
  }, [])

  return (
    <div className="splash">
      <div className="splash-hero">
        <div className="splash-diagram">
          <div className="board" ref={ref} />
        </div>
        <div className="splash-copy">
          <h1>
            You know your openings.
            <br />
            Do you know what comes next?
          </h1>
          <p>
            Middlegame Trainer quizzes you on positions from your own repertoire — real positions,
            from real club games, one move at a time. Build an opening tree, and the trainer finds
            the middlegames that actually grow out of it.
          </p>
          <p className="splash-detail">
            Positions come from the Lichess amateur database at your rating. Each quiz move is
            graded two ways: how often masters chose it, and what it does to the engine evaluation.
          </p>
          <a className="signin" href="/api/auth/login">
            Sign in with Lichess
          </a>
        </div>
      </div>
    </div>
  )
}
