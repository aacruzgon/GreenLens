import { useRef } from "react";
import { randomUUID } from "expo-crypto";
import { useMutation, type UseMutationOptions } from "@tanstack/react-query";

// docs/frontend 9, idempotency keys: a key belongs to a user action. Made when the user acts,
// reused for every retry of that action, dropped when it ends.

export function newKey(): string {
  return randomUUID();
}

export function useIdempotentMutation<TData, TVars>(
  fn: (vars: TVars, key: string) => Promise<TData>,
  options: Omit<UseMutationOptions<TData, Error, TVars>, "mutationFn"> = {},
) {
  const keyRef = useRef<string | null>(null);
  return useMutation<TData, Error, TVars>({
    ...options,
    mutationFn: (vars) => {
      keyRef.current ??= newKey();
      return fn(vars, keyRef.current);
    },
    onSettled: (...args) => {
      keyRef.current = null;
      return options.onSettled?.(...args);
    },
  });
}
