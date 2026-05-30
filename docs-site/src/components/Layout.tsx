import { ReactNode } from "react";
import Sidebar from "./Sidebar";

export default function Layout({ children }: { children: ReactNode }) {
  return (
    <div className="flex min-h-screen">
      <Sidebar />
      <main className="flex-1 lg:ml-64 min-w-0">
        <div className="max-w-3xl mx-auto px-6 sm:px-10 py-12 pb-20">{children}</div>
      </main>
    </div>
  );
}
