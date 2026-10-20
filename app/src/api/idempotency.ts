import { useMutation, type UseMutationOptions, type UseMutationResult } from "@tanstack/react-query";
import { useRef } from "react";

import { uuid } from "@/lib/uuid";

// A key belongs to a user action, not to a request (contracts §1, idempotency; README §9).
// It is made when the user acts, reused for every retry of that action with the same body,
// and dropped when the action ends. A changed body (a re-encoded photo, pruned pins, a chat
// resend) is a new action with a new key.

export type IdempotencyKey = string & { readonly __idempotencyKey: unique symbol };

export function newKey(): IdempotencyKey {
  return uuid() as IdempotencyKey;
}

/** The variables a keyed mutation runs with: the caller's, plus the key of this action. */
export interface Keyed<TVars> {
  vars: TVars;
  key: IdempotencyKey;
}

export type IdempotentMutationOptions<TData, TError, TVars, TContext> = Omit<
  UseMutationOptions<TData, TError, Keyed<TVars>, TContext>,
  "mutationFn"
> & {
  mutationFn: (vars: TVars, key: IdempotencyKey) => Promise<TData>;
};

export type IdempotentMutationResult<TData, TError, TVars, TContext> = Omit<
  UseMutationResult<TData, TError, Keyed<TVars>, TContext>,
  "mutate" | "mutateAsync"
> & {
  /** A new user action: makes a key, unless the caller already holds one (a capture draft's shot). */
  mutate: (vars: TVars, key?: IdempotencyKey) => void;
  mutateAsync: (vars: TVars, key?: IdempotencyKey) => Promise<TData>;
};

/**
 * useMutation with a key per user action. TanStack's retries call `mutationFn` again with the
 * same variables object, so every retry sends the same key and the same body. The key lives
 * only in that object, so it is gone once the mutation succeeds or finally fails.
 */
export function useIdempotentMutation<TData, TError = Error, TVars = void, TContext = unknown>(
  options: IdempotentMutationOptions<TData, TError, TVars, TContext>,
): IdempotentMutationResult<TData, TError, TVars, TContext> {
  const { mutationFn, ...rest } = options;
  const m = useMutation<TData, TError, Keyed<TVars>, TContext>({
    ...rest,
    mutationFn: ({ vars, key }) => mutationFn(vars, key),
  });
  return {
    ...m,
    mutate: (vars, key) => m.mutate({ vars, key: key ?? newKey() }),
    mutateAsync: (vars, key) => m.mutateAsync({ vars, key: key ?? newKey() }),
  };
}

/**
 * The key of one user action, kept while its body is unchanged (contracts 1): tapping Save again
 * after a lost response resends the same key, so the server answers with the stored result instead
 * of creating a second record. A changed body is a new action with a new key.
 */
export function useActionKey(): (body: unknown) => IdempotencyKey {
  const last = useRef<{ body: string; key: IdempotencyKey } | null>(null);
  return (body) => {
    const json = JSON.stringify(body);
    if (last.current?.body !== json) last.current = { body: json, key: newKey() };
    return last.current.key;
  };
}
