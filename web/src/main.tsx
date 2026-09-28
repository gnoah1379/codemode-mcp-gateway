import React from 'react'
import { createRoot } from 'react-dom/client'
import App from './App'
import { applyThemePreference, readThemePreference } from './hooks/useTheme'
import './styles/index.css'

// Apply the stored theme before the first render to avoid a flash of the wrong colors.
applyThemePreference(readThemePreference())

createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
)
