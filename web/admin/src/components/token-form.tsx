"use client";

import { FormEvent, useEffect, useState } from "react";
import { useSearchParams, useRouter } from "next/navigation";
import { useToken } from "./token-context";

interface TokenFormProps {
  label?: string;
  showClear?: boolean;
  onSaved?: (token: string) => void;
}

export function TokenForm({ label = "Daemon API token", showClear = true, onSaved }: TokenFormProps) {
  const { token, setToken } = useToken();
  const [value, setValue] = useState(token);
  const [masked, setMasked] = useState(true);
  const params = useSearchParams();
  const router = useRouter();

  useEffect(() => {
    setValue(token);
  }, [token]);

  const handleSubmit = (event: FormEvent) => {
    event.preventDefault();
    const next = value.trim();
    setToken(next);
    onSaved?.(next);
    const returnTo = params?.get("returnTo");
    if (returnTo && next) {
      router.replace(returnTo);
    }
  };

  return (
    <form onSubmit={handleSubmit} className="flex flex-wrap gap-3 items-end">
      <label className="flex flex-col text-sm">
        <span className="font-bold text-purple-300">{label}</span>
        <input
          type={masked ? "password" : "text"}
          value={value}
          onChange={(event) => setValue(event.target.value)}
          placeholder="Paste bearer token"
          className="mt-1 min-w-[16rem] rounded-lg border-2 border-[#75ffaf]/40 bg-[#1a1f3a] px-4 py-2 text-sm text-slate-200 placeholder:text-slate-500 focus:border-[#75ffaf] focus:ring-2 focus:ring-[#75ffaf] focus:shadow-lg focus:shadow-[#75ffaf]/20 focus:outline-none transition-all"
        />
      </label>
      <div className="flex gap-2">
        <button
          type="button"
          className="rounded-lg border-2 border-purple-500/50 bg-[#1a1f3a] px-4 py-2 text-sm font-semibold text-purple-300 hover:bg-purple-500/20 hover:border-purple-400 transition-all"
          onClick={() => setMasked((state) => !state)}
        >
          {masked ? "👁️ Show" : "🙈 Hide"}
        </button>
        <button
          type="submit"
          className="rounded-lg bg-gradient-to-r from-purple-600 to-pink-600 px-5 py-2 text-sm font-bold text-white shadow-lg shadow-purple-500/50 hover:shadow-purple-400/60 hover:from-purple-500 hover:to-pink-500 transition-all"
        >
          💾 Save token
        </button>
        {showClear ? (
          <button
            type="button"
            onClick={() => {
              setValue("");
              setToken("");
            }}
            className="rounded-lg border-2 border-slate-500/50 bg-[#1a1f3a] px-4 py-2 text-sm font-semibold text-slate-400 hover:bg-slate-700/50 hover:border-slate-400 transition-all"
          >
            🗑️ Clear
          </button>
        ) : null}
      </div>
    </form>
  );
}
