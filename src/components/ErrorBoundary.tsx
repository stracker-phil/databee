import { Component, type ErrorInfo, type ReactNode } from "react";
import { api } from "../api";

interface State {
  error: Error | null;
}

export class ErrorBoundary extends Component<{ children: ReactNode }, State> {
  state: State = { error: null };

  static getDerivedStateFromError(error: Error): State {
    return { error };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    void api.logError(`UI crash: ${error.message}\n${info.componentStack ?? ""}`).catch(() => {});
  }

  render() {
    if (!this.state.error) return this.props.children;
    return (
      <div className="crash" role="alert">
        <h2>Something went wrong</h2>
        <p className="muted">The error was written to the log. Your connections are still open.</p>
        <pre>{this.state.error.message}</pre>
        <div style={{ display: "flex", gap: 8 }}>
          <button className="btn btn-primary" onClick={() => this.setState({ error: null })}>
            Try again
          </button>
          <button className="btn" onClick={() => void api.openLogFolder()}>
            Open log folder
          </button>
        </div>
      </div>
    );
  }
}
