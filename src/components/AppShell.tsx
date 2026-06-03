import type { ReactNode } from "react";

interface AppShellProps {
  children: ReactNode;
}

export function AppShell({ children }: AppShellProps) {
  return (
    <main className="app-shell">
      <div className="app-shell__inner">{children}</div>
    </main>
  );
}
