import { useRouter } from "expo-router";
import { useState } from "react";
import { Alert } from "react-native";

import { isApiError, useDeleteGarden, useGarden, useUpdateGarden } from "@/api";
import { can } from "@/domain/permissions";

import { locationLine } from "../model/members";
import { useLocationInput } from "./useLocationInput";

// gardens/[gardenId]/edit (screens 3.9): the owner renames the garden, changes where it is, or deletes it.
export function useGardenEdit(gardenId: string) {
  const router = useRouter();
  const garden = useGarden(gardenId);
  const update = useUpdateGarden(gardenId);
  const remove = useDeleteGarden();
  const where = useLocationInput();
  const [name, setName] = useState<string | null>(null);
  const [changingPlace, setChangingPlace] = useState(false);
  const [problem, setProblem] = useState<string>();

  const g = garden.data;
  const shownName = name ?? g?.name ?? "";
  const nameChanged = Boolean(g && shownName.trim() && shownName.trim() !== g.name);

  const why = (e: unknown) => (isApiError(e) ? e.message : "Check your connection and try again.");

  const saveName = () => {
    setProblem(undefined);
    update.mutateAsync({ name: shownName.trim() }).then(() => setName(null), (e: unknown) => setProblem(why(e)));
  };

  const saveLocation = () => {
    if (!where.location) return;
    setProblem(undefined);
    update.mutateAsync({ location: where.location }).then(
      () => setChangingPlace(false),
      (e: unknown) => setProblem(why(e)),
    );
  };

  const deleteGarden = () =>
    Alert.alert(`Delete ${g?.name ?? "this garden"}?`, "Its plants, scans, photos, tasks and chats are deleted for every member. This can't be undone.", [
      { text: "Cancel", style: "cancel" },
      {
        text: "Delete",
        style: "destructive",
        // The session gate moves to another garden, or onboarding, once the list refetches without this one.
        onPress: () => remove.mutateAsync(gardenId).then(() => router.dismissTo("/"), (e: unknown) => setProblem(why(e))),
      },
    ]);

  return {
    garden: g,
    loading: garden.isPending,
    error: garden.error,
    retry: () => void garden.refetch(),
    isOwner: can(g?.my_role, "garden.manage"),
    name: shownName,
    setName,
    nameChanged,
    location: g ? locationLine(g) : "",
    where,
    changingPlace,
    setChangingPlace,
    saving: update.isPending,
    deleting: remove.isPending,
    problem,
    actions: {
      saveName,
      saveLocation,
      deleteGarden,
      back: () => router.back(),
      members: () => router.push({ pathname: "/gardens/[gardenId]/members", params: { gardenId } }),
      connectors: () => router.push({ pathname: "/gardens/[gardenId]/connectors", params: { gardenId } }),
    },
  };
}
