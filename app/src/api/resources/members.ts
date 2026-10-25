import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import { client, ok } from "../client";
import type { CreateInviteRequest, Garden, Invite, Member, Page, Role } from "@/domain/types";
import { useIdempotentMutation, type IdempotencyKey } from "../idempotency";
import { fetchAllPages } from "../pages";
import { qk } from "../queryKeys";

// Members and invites (contracts 9; flows 3). Members join with invite codes, never by email.

export const members = {
  list: async (gardenId: string, cursor?: string): Promise<Page<Member>> =>
    ok(await client().GET("/gardens/{id}/members", { params: { path: { id: gardenId }, query: { cursor } } })),
  /** Owner. Making someone owner adds an owner; demoting the last owner is `409 invalid_state`. */
  setRole: async (gardenId: string, userId: string, role: Role): Promise<Member> =>
    ok(
      await client().PATCH("/gardens/{id}/members/{user_id}", {
        params: { path: { id: gardenId, user_id: userId } },
        body: { role },
      }),
    ),
  /** An owner removing anyone, or a member leaving. The last owner can't leave: `409 invalid_state`. */
  remove: async (gardenId: string, userId: string): Promise<void> => {
    await client().DELETE("/gardens/{id}/members/{user_id}", { params: { path: { id: gardenId, user_id: userId } } });
  },
};

export const invites = {
  list: async (gardenId: string, cursor?: string): Promise<Page<Invite>> =>
    ok(await client().GET("/gardens/{id}/invites", { params: { path: { id: gardenId }, query: { cursor } } })),
  create: async (gardenId: string, body: CreateInviteRequest, key: IdempotencyKey): Promise<Invite> =>
    ok(
      await client().POST("/gardens/{id}/invites", {
        params: { path: { id: gardenId }, header: { "Idempotency-Key": key } },
        body,
      }),
    ),
  revoke: async (gardenId: string, inviteId: string): Promise<void> => {
    await client().DELETE("/gardens/{id}/invites/{invite_id}", { params: { path: { id: gardenId, invite_id: inviteId } } });
  },
  /** `200 Garden`; `404` for an unknown, used, revoked or expired code; `429` after 10 tries a minute. */
  accept: async (code: string): Promise<Garden> =>
    ok(await client().POST("/invites/{code}/accept", { params: { path: { code } } })),
};

/** Every member of the garden, by `since`. Any member. */
export function useMembers(gardenId: string) {
  return useQuery({
    queryKey: qk.members(gardenId),
    queryFn: () => fetchAllPages((cursor) => members.list(gardenId, cursor)),
    enabled: gardenId !== "",
  });
}

/** The garden's open invites, newest first. Owners only, so the caller passes whether to ask. */
export function useInvites(gardenId: string, enabled: boolean) {
  return useQuery({
    queryKey: qk.invites(gardenId),
    queryFn: () => fetchAllPages((cursor) => invites.list(gardenId, cursor)),
    enabled: enabled && gardenId !== "",
  });
}

export function useCreateInvite(gardenId: string) {
  const qc = useQueryClient();
  return useIdempotentMutation({
    mutationFn: (body: CreateInviteRequest, key) => invites.create(gardenId, body, key),
    onSuccess: () => qc.invalidateQueries({ queryKey: qk.invites(gardenId) }),
  });
}

export function useRevokeInvite(gardenId: string) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (inviteId: string) => invites.revoke(gardenId, inviteId),
    onSuccess: () => qc.invalidateQueries({ queryKey: qk.invites(gardenId) }),
  });
}

export function useSetMemberRole(gardenId: string) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ userId, role }: { userId: string; role: Role }) => members.setRole(gardenId, userId, role),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: qk.garden(gardenId) });
      return qc.invalidateQueries({ queryKey: qk.members(gardenId) });
    },
  });
}

/** Removes a member, or the caller leaving; a caller who left refetches the gardens without this one. */
export function useRemoveMember(gardenId: string) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (userId: string) => members.remove(gardenId, userId),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: qk.gardens() });
      return qc.invalidateQueries({ queryKey: qk.members(gardenId) });
    },
  });
}

/** Accepts an invite; the caller makes the garden active (flows 3). */
export function useAcceptInvite() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (code: string) => invites.accept(code),
    onSuccess: (garden) => {
      qc.setQueryData(qk.garden(garden.id), garden);
      return qc.invalidateQueries({ queryKey: qk.gardens() });
    },
  });
}
