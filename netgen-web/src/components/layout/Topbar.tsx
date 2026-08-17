'use client'

import { useApp } from '@/lib/store'
import { downloadExport } from '@/lib/api'
import { useState } from 'react'

// ─── Logo ─────────────────────────────────────────────────────────────────────

function Logo() {
  return (
    <svg width="22" height="22" viewBox="0 0 22 22" fill="none">
      <path d="M11 2L19.5 7V15L11 20L2.5 15V7L11 2Z"
        fill="#1a1a18" stroke="#1a1a18" strokeWidth="1.2" strokeLinejoin="round"/>
      <line x1="11" y1="2" x2="11" y2="20" stroke="#f5f4f1" strokeWidth="0.8" opacity="0.4"/>
      <line x1="2.5" y1="7" x2="19.5" y2="7" stroke="#f5f4f1" strokeWidth="0.8" opacity="0.4"/>
      <line x1="2.5" y1="15" x2="19.5" y2="15" stroke="#f5f4f1" strokeWidth="0.8" opacity="0.4"/>
    </svg>
  )
}

// ─── Topbar ───────────────────────────────────────────────────────────────────

export default function Topbar() {
  const { state, dispatch } = useApp()
  const [avatarHovered, setAvatarHovered] = useState(false)

  const isGuest = !state.userEmail || state.userEmail === 'guest'
  const isLoggedIn = !isGuest

  function signOut() {
    localStorage.removeItem('laby_token')
    localStorage.removeItem('laby_email')
    dispatch({ type: 'AUTH_CLEAR' })
  }

  function signIn() {
    // Clear the guest sentinel so the auth modal shows
    dispatch({ type: 'AUTH_CLEAR' })
  }

  const initials = isLoggedIn
    ? state.userEmail!.slice(0, 2).toUpperCase()
    : 'G'

  return (
    <header style={{
      height: 44,
      background: '#ffffff',
      borderBottom: '1px solid #e8e6e0',
      display: 'flex',
      alignItems: 'center',
      padding: '0 14px',
      gap: 10,
      flexShrink: 0,
    }}>
      {/* Left */}
      <div style={{ display: 'flex', alignItems: 'center', gap: 8, flex: 1 }}>
        <Logo />
        <span style={{ fontSize: 14, fontWeight: 600, color: '#1a1a18', letterSpacing: '-0.02em' }}>
          Labyrinth
        </span>

        {state.project && (
          <>
            <span style={{ color: '#d8d6cc', fontSize: 14 }}>/</span>
            <span style={{ fontSize: 13, fontWeight: 500, color: '#5f5e5a', maxWidth: 200, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
              {state.project.name}
            </span>
            <svg width="12" height="12" viewBox="0 0 12 12" fill="none">
              <path d="M3 4.5L6 7.5L9 4.5" stroke="#888780" strokeWidth="1.2" strokeLinecap="round" strokeLinejoin="round"/>
            </svg>
          </>
        )}

        {state.project?.status === 'ready' && (
          <div style={{
            display: 'flex', alignItems: 'center', gap: 5,
            background: '#edf7f2', borderRadius: 9999, padding: '2px 8px',
          }}>
            <div style={{ width: 5, height: 5, borderRadius: '50%', background: '#1d9e75' }} />
            <span style={{ fontSize: 11, fontWeight: 500, color: '#0f6e56' }}>Saved</span>
          </div>
        )}

        {state.project?.status === 'generating' && (
          <div style={{
            display: 'flex', alignItems: 'center', gap: 5,
            background: '#faeeda', borderRadius: 9999, padding: '2px 8px',
          }}>
            <div style={{ width: 5, height: 5, borderRadius: '50%', background: '#ef9f27' }} />
            <span style={{ fontSize: 11, fontWeight: 500, color: '#854f0b' }}>Building…</span>
          </div>
        )}
      </div>

      {/* Right */}
      <div style={{ display: 'flex', alignItems: 'center', gap: 6 }}>
        {state.project?.status === 'ready' && (
          <button
            onClick={() => downloadExport(state.project!.id, 'svg', `${state.project!.name}.svg`)}
            style={{
              height: 30, padding: '0 12px',
              background: '#1a1a18', color: '#ffffff',
              border: 'none', borderRadius: 7,
              fontSize: 12, fontWeight: 500,
              cursor: 'pointer', display: 'flex', alignItems: 'center', gap: 6,
            }}
          >
            <svg width="12" height="12" viewBox="0 0 12 12" fill="none">
              <path d="M6 1v7M3 5.5L6 8.5L9 5.5M2 9.5h8" stroke="white" strokeWidth="1.3" strokeLinecap="round" strokeLinejoin="round"/>
            </svg>
            Export
          </button>
        )}

        <div style={{ position: 'relative' }}>
          <button
            title={isLoggedIn ? `${state.userEmail} — click to sign out` : 'Sign in or create account'}
            onClick={isLoggedIn ? signOut : signIn}
            onMouseEnter={() => setAvatarHovered(true)}
            onMouseLeave={() => setAvatarHovered(false)}
            style={{
              width: 26, height: 26, borderRadius: '50%',
              background: isLoggedIn ? '#1a1a18' : '#d8d6cc',
              color: '#ffffff',
              border: 'none',
              display: 'flex', alignItems: 'center', justifyContent: 'center',
              fontSize: 10, fontWeight: 600, cursor: 'pointer',
              flexShrink: 0,
            }}
          >
            {initials}
          </button>
          {avatarHovered && (
            <div style={{
              position: 'absolute', right: 0, top: 34,
              background: '#1a1a18', color: '#ffffff',
              fontSize: 11, fontWeight: 500, whiteSpace: 'nowrap',
              padding: '5px 9px', borderRadius: 6,
              pointerEvents: 'none', zIndex: 9998,
            }}>
              {isLoggedIn ? 'Sign out' : 'Sign in / create account'}
            </div>
          )}
        </div>
      </div>
    </header>
  )
}
