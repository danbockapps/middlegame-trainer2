import {useEffect, useState} from 'react'
import {BrowserRouter, Link, Route, Routes} from 'react-router-dom'
import {getMe, logout} from './api/client'
import type {User} from './api/types'
import {Splash} from './pages/Splash'
import {Openings} from './pages/Openings'
import {Editor} from './pages/Editor'
import {Quiz} from './pages/Quiz'

export function App() {
  const [user, setUser] = useState<User | null | undefined>(undefined)

  useEffect(() => {
    document.title = 'Middlegame Trainer'
    getMe().then(setUser)
  }, [])

  if (user === undefined) return <p className="loading">Loading…</p>
  if (user === null) return <Splash />

  return (
    <BrowserRouter>
      <div className="page">
        <header className="masthead">
          <div className="masthead-inner">
            <Link to="/" className="wordmark">
              Middlegame Trainer
            </Link>
            <span className="masthead-user">
              {user.username}
              <button
                type="button"
                className="link-button"
                onClick={() => logout().then(() => setUser(null))}
              >
                Sign out
              </button>
            </span>
          </div>
        </header>
        <main className="main">
          <Routes>
            <Route path="/" element={<Openings />} />
            <Route path="/openings/:id" element={<Editor key="editor" />} />
            <Route path="/quiz/:id" element={<Quiz user={user} />} />
          </Routes>
        </main>
      </div>
    </BrowserRouter>
  )
}
