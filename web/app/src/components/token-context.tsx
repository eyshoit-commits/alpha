"use client";

import { createContext, useContext, useMemo, useState } from "react";
import { deleteTokenCookie, NAMESPACE_TOKEN_COOKIE, writeTokenCookie } from "@shared/auth";

type TokenContextValue = {
  token: string;
  setToken: (value: string) => void;
};

const TokenContext = createContext<TokenContextValue | undefined>(undefined);

interface TokenProviderProps {
  children: React.ReactNode;
  initialToken?: string;
}

export function TokenProvider({ children, initialToken = "" }: TokenProviderProps) {
  const [token, setTokenState] = useState(initialToken);

  const setToken = (value: string) => {
    const next = value.trim();
    setTokenState(next);
    if (next) {
      writeTokenCookie(NAMESPACE_TOKEN_COOKIE, next);
    } else {
      deleteTokenCookie(NAMESPACE_TOKEN_COOKIE);
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
