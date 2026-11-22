// Typed client for docs/api-contract.md. Cookie-session auth: always send credentials.

import {
  RequestError,
  type ApiError,
  type Opening,
  type OpeningSummary,
  type QuizEval,
  type QuizMoveResult,
  type QuizStart,
  type SearchSettings,
  type Side,
  type User,
} from './types'

async function request<T>(path: string, init?: RequestInit): Promise<T> {
  const res = await fetch(path, {
    credentials: 'same-origin',
    headers: init?.body ? {'Content-Type': 'application/json'} : undefined,
    ...init,
  })
  if (!res.ok) {
    let body: ApiError | null = null
    try {
      body = (await res.json()) as ApiError
    } catch {
      body = null
    }
    throw new RequestError(res.status, body)
  }
  if (res.status === 204) return undefined as T
  return (await res.json()) as T
}

function post<T>(path: string, body?: unknown): Promise<T> {
  return request<T>(path, {
    method: 'POST',
    body: body === undefined ? undefined : JSON.stringify(body),
  })
}

function put<T>(path: string, body: unknown): Promise<T> {
  return request<T>(path, {method: 'PUT', body: JSON.stringify(body)})
}

// --- Auth ---

export function getMe(): Promise<User | null> {
  return fetch('/api/me', {credentials: 'same-origin'}).then((res) =>
    res.ok ? (res.json() as Promise<User>) : null,
  )
}

export function logout(): Promise<void> {
  return post<void>('/api/auth/logout')
}

// --- Openings ---

export function listOpenings(deleted: boolean): Promise<OpeningSummary[]> {
  return request<OpeningSummary[]>(`/api/openings${deleted ? '?deleted=1' : ''}`)
}

export function createOpening(side: Side): Promise<OpeningSummary> {
  return post<OpeningSummary>('/api/openings', {side})
}

export function getOpening(id: number): Promise<Opening> {
  return request<Opening>(`/api/openings/${id}`)
}

export interface OpeningUpdate {
  moves: string[]
  nameOverride: string | null
  rangeMin: number
  rangeMax: number
}

export function updateOpening(id: number, body: OpeningUpdate): Promise<Opening> {
  return put<Opening>(`/api/openings/${id}`, body)
}

export function deleteOpening(id: number): Promise<void> {
  return request<void>(`/api/openings/${id}`, {method: 'DELETE'})
}

export function restoreOpening(id: number): Promise<void> {
  return post<void>(`/api/openings/${id}/restore`)
}

export function deleteOpeningPermanently(id: number): Promise<void> {
  return request<void>(`/api/openings/${id}/permanent`, {method: 'DELETE'})
}

// --- Quiz ---

export function startQuiz(openingId: number): Promise<QuizStart> {
  return post<QuizStart>('/api/quiz/start', {openingId})
}

export function playQuizMove(quizId: string, uci: string): Promise<QuizMoveResult> {
  return post<QuizMoveResult>(`/api/quiz/${quizId}/move`, {uci})
}

export function skipQuiz(quizId: string): Promise<QuizStart> {
  return post<QuizStart>(`/api/quiz/${quizId}/skip`)
}

// --- Search settings ---

export function getSettings(): Promise<SearchSettings | null> {
  return request<SearchSettings | null>('/api/settings').catch(() => null)
}

export function saveSettings(settings: SearchSettings): Promise<SearchSettings> {
  return put<SearchSettings>('/api/settings', settings)
}

// --- Raw eval shaping ---

interface RawEval {
  before?: number | null
  after?: number | null
  mate?: number | null
  source?: string
}

/**
 * The contract encodes mate scores as `"mate": n` in place of before/after numbers.
 * Normalize both shapes into explicit nullable fields so the UI never guesses.
 * (Shape is narrowed here rather than in the UI — the one place `unknown` is unpacked.)
 */
export function shapeEval(raw: unknown): QuizEval {
  const e = (raw ?? {}) as RawEval
  const mate = typeof e.mate === 'number' ? e.mate : null
  return {
    before: typeof e.before === 'number' ? e.before : null,
    after: typeof e.after === 'number' ? e.after : null,
    mate,
    source: e.source ?? 'unknown',
  }
}
