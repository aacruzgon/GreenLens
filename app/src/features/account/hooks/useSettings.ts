import { useRouter } from "expo-router";
import { useState } from "react";
import { Alert } from "react-native";

import { flattenPages, isApiError, useGardens, useMe, useUpdateMe } from "@/api";
import { can } from "@/domain/permissions";
import type { Garden } from "@/domain/types";
import { useSession } from "@/features/auth";
import { useActiveGardenStore } from "@/features/gardens";

export interface GardenRow {
  garden: Garden;
  active: boolean;
  /** "owner · open now". */
  subtitle: string;
  /** Owners manage the garden; everyone else sees its members. */
  manage: { label: "Manage" | "Members"; onPress: () => void };
}

// Settings (screens 3.9): the account, the gardens with switching, Sign out and Delete account.
export function useSettings() {
  const router = useRouter();
  const session = useSession();
  const me = useMe();
  const updateMe = useUpdateMe();
  const gardens = useGardens();
  const activeId = useActiveGardenStore((s) => s.gardenId);
  const setGardenId = useActiveGardenStore((s) => s.setGardenId);
  const [editingName, setEditingName] = useState(false);
  const [name, setName] = useState("");
  const [signingOut, setSigningOut] = useState(false);
  const [problem, setProblem] = useState<string>();

  const failed = (e: unknown) => setProblem(isApiError(e) ? e.message : "That didn't save. Try again.");

  const rows: GardenRow[] = flattenPages(gardens.data).map((garden) => {
    const active = garden.id === activeId;
    const owner = can(garden.my_role, "garden.manage");
    return {
      garden,
      active,
      subtitle: active ? `${garden.my_role} · open now` : garden.my_role,
      manage: owner
        ? { label: "Manage", onPress: () => router.push({ pathname: "/gardens/[gardenId]/edit", params: { gardenId: garden.id } }) }
        : { label: "Members", onPress: () => router.push({ pathname: "/gardens/[gardenId]/members", params: { gardenId: garden.id } }) },
    };
  });

  /** Switches the active garden and shows its home (flows 1: the active garden is the one chosen last). */
  const open = (gardenId: string) => {
    setGardenId(gardenId);
    router.navigate("/");
  };

  const startEditingName = () => {
    setName(me.data?.display_name ?? "");
    setEditingName(true);
  };

  const saveName = () => {
    const value = name.trim();
    if (!value) return;
    setProblem(undefined);
    updateMe.mutateAsync({ display_name: value }).then(() => setEditingName(false), failed);
  };

  const setConsent = (on: boolean) => {
    const send = () => {
      setProblem(undefined);
      updateMe.mutateAsync({ location_consent: on }).catch(failed);
    };
    if (on) return send();
    // Withdrawing rounds every garden the user owns to about 10 km (contracts 9, account); say so first.
    Alert.alert("Stop sharing your location?", "Every garden you own will be rounded to about 10 km.", [
      { text: "Cancel", style: "cancel" },
      { text: "Turn off", onPress: send },
    ]);
  };

  // The session layer clears the cache and client state on every sign-out (features/shell).
  const signOut = async () => {
    setSigningOut(true);
    await session.signOut();
    setSigningOut(false);
  };

  return {
    session,
    me: me.data,
    rows,
    gardensLoading: gardens.isPending,
    editingName,
    name,
    setName,
    savingName: updateMe.isPending,
    problem,
    signingOut,
    actions: {
      open,
      startEditingName,
      cancelEditingName: () => setEditingName(false),
      saveName,
      setConsent,
      signOut,
      newGarden: () => router.push("/gardens/new"),
      deleteAccount: () => router.push("/account/delete"),
    },
  };
}
