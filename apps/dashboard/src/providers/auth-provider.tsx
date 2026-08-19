"use client";

import {
  createContext,
  useContext,
  useEffect,
  useMemo,
  useState,
} from "react";

import type { AuthResponse } from "@/types/auth";
import { logout as logoutRequest } from "@/lib/auth-api";
import {
  clearTokens,
  isMarkedAuthenticated,
  markAuthenticated,
} from "@/lib/auth";

type AuthContextValue = {
  session: AuthResponse | null;
  authenticated: boolean;
  loading: boolean;

  setSession: (session: AuthResponse | null) => void;
  logout: () => void;
};

const AuthContext = createContext<AuthContextValue | null>(null);

export function AuthProvider({
  children,
}: {
  children: React.ReactNode;
}) {
  const [session, setSessionState] =
    useState<AuthResponse | null>(null);

  const [loading, setLoading] = useState(true);

  useEffect(() => {
    // Ba mu iya karanta access_token din kai tsaye (httpOnly cookie ce),
    // don haka mun dogara ga UI flag kawai domin sanin ko an login. Kiran
    // authenticated endpoint na farko zai kori user zuwa /login ta hanyar
    // 401 interceptor idan cookie ba ta wanzu ba ko ta kare.
    if (isMarkedAuthenticated()) {
      setSessionState({ access_token: "" });
    }

    setLoading(false);
  }, []);

  function setSession(session: AuthResponse | null) {
    if (!session) {
      clearTokens();
      setSessionState(null);
      return;
    }

    markAuthenticated();
    setSessionState(session);
  }

  function logout() {
    // Clear local UI state immediately so the user is kicked back to the
    // login screen right away, then ask the server to clear the httpOnly
    // access_token cookie. Previously this only cleared a localStorage
    // flag — the cookie (and the JWT inside it) stayed valid until its
    // 24h expiry, so "logout" on a shared machine didn't actually log
    // anyone out. Errors here are swallowed on purpose: even if the
    // network call fails, the user must not be left stuck on a "logout"
    // action that appears to do nothing.
    clearTokens();
    setSessionState(null);
    void logoutRequest().catch(() => {});
  }

  const value = useMemo<AuthContextValue>(
    () => ({
      session,
      authenticated: !!session,
      loading,
      setSession,
      logout,
    }),
    [session, loading],
  );

  return (
    <AuthContext.Provider value={value}>
      {children}
    </AuthContext.Provider>
  );
}

export function useAuthContext() {
  const ctx = useContext(AuthContext);

  if (!ctx) {
    throw new Error(
      "useAuthContext must be used inside AuthProvider",
    );
  }

  return ctx;
}
