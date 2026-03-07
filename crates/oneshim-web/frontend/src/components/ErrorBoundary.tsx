import { Component, type ErrorInfo, type ReactNode } from 'react'
import { withTranslation, type WithTranslation } from 'react-i18next'

// ErrorBoundary Props: children + optional fallback + i18next HOC props
interface OwnProps {
  children: ReactNode
  fallback?: ReactNode
}

type Props = OwnProps & WithTranslation

interface State {
  hasError: boolean
  error: Error | null
}

// 클래스 컴포넌트는 React Error Boundary 필수 조건이므로 withTranslation HOC로 i18n 연동
class ErrorBoundaryBase extends Component<Props, State> {
  constructor(props: Props) {
    super(props)
    this.state = { hasError: false, error: null }
  }

  static getDerivedStateFromError(error: Error): State {
    return { hasError: true, error }
  }

  componentDidCatch(error: Error, errorInfo: ErrorInfo) {
    console.error('ErrorBoundary caught:', error, errorInfo)
  }

  render() {
    const { t, fallback, children } = this.props

    if (this.state.hasError) {
      return (
        fallback || (
          <div className="flex min-h-screen items-center justify-center bg-surface-muted">
            <div className="p-8 text-center">
              <h1 className="mb-4 font-bold text-2xl text-red-600">{t('errors.boundary_title')}</h1>
              <p className="mb-4 text-content-secondary">{this.state.error?.message}</p>
              <button
                type="button"
                onClick={() => this.setState({ hasError: false, error: null })}
                className="rounded bg-blue-600 px-4 py-2 text-white hover:bg-blue-700"
              >
                {t('errors.boundary_retry')}
              </button>
            </div>
          </div>
        )
      )
    }

    return children
  }
}

// withTranslation HOC로 감싸 번역 함수 t()를 class 컴포넌트에 주입
export default withTranslation()(ErrorBoundaryBase)
