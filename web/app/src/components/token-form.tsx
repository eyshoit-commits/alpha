"use client";

import { FormEvent, useState } from "react";
import { useToken } from "./token-context";

export function TokenForm() {
  const { token, setToken } = useToken();
  const [value, setValue] = useState(token);

  const handleSubmit = (event: FormEvent) => {
    event.preventDefault();
    setToken(value.trim());
  };

  return (
    <form onSubmit={handleSubmit} className="flex flex-wrap gap-3 items-end text-sm">
      <label className="flex flex-col">
        <span className="font-medium">Namespace token</span>
        <input
          type="password"
          value={value}
          onChange={(event) => setValue(event.target.value)}
          placeholder="Paste bearer token"
          className="mt-1 min-w-[16rem] rounded-md border border-slate-300 px-3 py-2 shadow-sm"
        />
      </label>
      <div className="flex gap-2">
        <button
          type="submit"
          className="rounded-md bg-slate-900 px-4 py-2 font-semibold text-white shadow-sm hover:bg-slate-700"
        >
          Save token
        </button>
        <button
          type="button"
          className="rounded-md border border-slate-200 bg-white px-3 py-2 shadow-sm"
          onClick={() => {
            setValue("");
            setToken("");
          }}
        >
          Clear
        </button>
      </div>
    </form>
  );
}
