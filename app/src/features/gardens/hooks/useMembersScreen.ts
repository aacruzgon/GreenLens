import { useRouter } from "expo-router";
import { Alert, Share } from "react-native";

import { isApiError, useCreateInvite, useGarden, useInvites, useMe, useMembers, useRemoveMember, useRevokeInvite, useSetMemberRole } from "@/api";
import { can } from "@/domain/permissions";
import type { Invite, Member, Role } from "@/domain/types";

import { LEAVE_WARNING, REMOVE_WARNING, ROLE_ORDER, canLeave, inviteMessage, inviteSubtitle, memberRows, roleName, type MemberRow } from "../model/members";

export interface InviteRow {
  invite: Invite;
  subtitle: string;
}

// gardens/[gardenId]/members (screens 3.9; flows 3): members with owner controls, open invites, and leaving.
export function useMembersScreen(gardenId: string) {
  const router = useRouter();
  const me = useMe();
  const garden = useGarden(gardenId);
  const members = useMembers(gardenId);
  const isOwner = can(garden.data?.my_role, "members.manage");
  const invites = useInvites(gardenId, isOwner);
  const createInvite = useCreateInvite(gardenId);
  const revoke = useRevokeInvite(gardenId);
  const setRole = useSetMemberRole(gardenId);
  const remove = useRemoveMember(gardenId);

  const meId = me.data?.id ?? "";
  const timeZone = garden.data?.time_zone ?? "UTC";
  const rows: MemberRow[] = members.data ? memberRows(members.data, meId, garden.data?.my_role, timeZone) : [];
  const inviteRows: InviteRow[] = (invites.data ?? []).map((invite) => ({ invite, subtitle: inviteSubtitle(invite, timeZone) }));

  const failed = (what: string) => (e: unknown) =>
    Alert.alert(what, isApiError(e) ? e.message : "Check your connection and try again.");

  /** The system share sheet with the link (HIG Collaboration and sharing). */
  const share = async (invite: Invite) => {
    await Share.share({ message: inviteMessage(garden.data?.name ?? "my garden", invite) }).catch(() => undefined);
  };

  const invite = (role: Role) =>
    createInvite.mutateAsync({ role }).then(share, failed("The invite wasn't made"));

  const chooseInviteRole = () =>
    Alert.alert("Invite as", "The link works once and for 7 days.", [
      ...ROLE_ORDER.map((role) => ({ text: roleName(role), onPress: () => void invite(role) })),
      { text: "Cancel", style: "cancel" as const },
    ]);

  const changeRole = (m: Member) =>
    Alert.alert(`${m.display_name}'s role`, undefined, [
      ...ROLE_ORDER.filter((r) => r !== m.role).map((role) => ({
        text: roleName(role),
        onPress: () => setRole.mutateAsync({ userId: m.user_id, role }).catch(failed("The role didn't change")),
      })),
      { text: "Cancel", style: "cancel" as const },
    ]);

  const removeMember = (m: Member) =>
    Alert.alert(`Remove ${m.display_name}?`, REMOVE_WARNING, [
      { text: "Cancel", style: "cancel" },
      { text: "Remove", style: "destructive", onPress: () => remove.mutateAsync(m.user_id).catch(failed("They weren't removed")) },
    ]);

  const revokeInvite = (i: Invite) =>
    Alert.alert("Revoke this invite?", "The link stops working.", [
      { text: "Cancel", style: "cancel" },
      { text: "Revoke", style: "destructive", onPress: () => revoke.mutateAsync(i.id).catch(failed("The invite wasn't revoked")) },
    ]);

  const leave = () =>
    Alert.alert(`Leave ${garden.data?.name ?? "this garden"}?`, LEAVE_WARNING, [
      { text: "Cancel", style: "cancel" },
      {
        text: "Leave",
        style: "destructive",
        // The session gate moves to another garden, or onboarding, once the list refetches without this one.
        onPress: () => remove.mutateAsync(meId).then(() => router.dismissTo("/"), failed("You're still a member")),
      },
    ]);

  return {
    garden: garden.data,
    loading: members.isPending || garden.isPending,
    error: members.error ?? garden.error,
    retry: () => {
      void members.refetch();
      void garden.refetch();
    },
    rows,
    isOwner,
    inviteRows,
    invitesLoading: isOwner && invites.isPending,
    inviting: createInvite.isPending,
    canLeave: members.data ? canLeave(members.data, meId) : false,
    actions: { chooseInviteRole, share, changeRole, removeMember, revokeInvite, leave, back: () => router.back() },
  };
}
