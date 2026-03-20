import React from 'react'
import ReactDOM from 'react-dom/client'
import OverlayApp from './App'
import './index.css'

// biome-ignore lint/style/noNonNullAssertion: overlay-root element is guaranteed to exist in overlay.html
ReactDOM.createRoot(document.getElementById('overlay-root')!).render(
  <React.StrictMode>
    <OverlayApp />
  </React.StrictMode>,
)
