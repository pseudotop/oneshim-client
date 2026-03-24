import { createRoot } from 'react-dom/client'
import { App } from './App'
import '../index.css'

// biome-ignore lint/style/noNonNullAssertion: root element guaranteed by panel.html
createRoot(document.getElementById('panel-root')!).render(<App />)
