"use client";

import { createContext, useContext, useEffect, useMemo, useState } from "react";

type TokenContextValue = {
  token: string;
  setToken: (value: string) => void;
};

const TokenContext = createContext<TokenContextValue | undefined>(undefined);
const STORAGE_KEY = "namespace-daemon-token";

export function TokenProvider({ children }: { children: React.ReactNode }) {
  const [token, setTokenState] = useState("");

  useEffect(() => {
    if (typeof window === "undefined") {
      return;
    }
    const stored = window.sessionStorage.getItem(STORAGE_KEY);
    if (stored) {
      setTokenState(stored);
    }
  }, []);

  const setToken = (value: string) => {
    setTokenState(value);
    if (typeof window !== "undefined") {
      if (value) {
        window.sessionStorage.setItem(STORAGE_KEY, value);
      } else {
        window.sessionStorage.removeItem(STORAGE_KEY);
      }
    }
  };

  const value = useMemo(() => ({ token, setToken }), [token]);

  return <TokenContext.Provider value={value}>{children}</TokenContext.Provider>;
}

export function useToken() {
  const context = useContext(TokenContext);
  if (!context) {
    throw new Error("useToken must be used within a TokenProvider");
  }
  return context;
}
