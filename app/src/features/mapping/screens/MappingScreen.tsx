// The garden mapping session (roadmap F5 item 5; architecture 6.8; flows 11; screens 3.8, Edit). The camera full
// screen and dark like capture: tap each bed's corners on the ground, then each plant to drop its pin. The card at
// the bottom says what a tap does now; Save uploads a whole new version of the map.
import { StatusBar } from "expo-status-bar";
import { useRef, useState } from "react";
import { ActivityIndicator, Alert, Pressable, ScrollView, View } from "react-native";
import { useSafeAreaInsets } from "react-native-safe-area-context";

import { useActiveGardenId } from "@/features/gardens";
import { GardenMappingView, useCapabilities, type GardenMappingHandle } from "@/platform";
import { Button, Text, ThemeProvider, camera, minTouch, space } from "@/ui";

import { useMappingSession } from "../hooks/useMappingSession";
import { openBed, openBedArea, sessionSummary } from "../model/session";

type Tap = "corner" | "pin";

const TRACKING_HINT = {
  normal: undefined,
  limited: "Move the phone slowly over the ground",
  lost: "Lost your place. Point the phone at the ground",
} as const;

export function MappingScreen() {
  return (
    <ThemeProvider scheme="dark">
      <Mapping />
    </ThemeProvider>
  );
}

function Mapping() {
  const gardenId = useActiveGardenId();
  const view = useRef<GardenMappingHandle>(null);
  const session = useMappingSession(gardenId, view);
  const { mapping: canMapHere } = useCapabilities();
  const insets = useSafeAreaInsets();
  const [tap, setTap] = useState<Tap>("corner");
  // ARKit couldn't start (camera access refused, a session error): said once, never a black screen (screens 4).
  const [mountError, setMountError] = useState<string>();
  const { phase, state, actions } = session;

  if (!GardenMappingView || !canMapHere) {
    return <Message text="Mapping needs an iPhone or iPad with LiDAR." onClose={actions.close} />;
  }
  if (phase.type === "loading") return <Message text="" busy onClose={actions.close} />;
  if (phase.type === "failed") {
    return <Message text="Couldn't load the garden's map." onClose={actions.close} retry={actions.retry} retrying={session.retrying} />;
  }
  if (!session.canMap) return <Message text="Only the garden's editors and owner can map it." onClose={actions.close} />;
  if (mountError !== undefined) return <Message text={mountError} onClose={actions.close} />;
  if (phase.type === "saved") {
    const dropped =
      phase.dropped === 0 ? "" : ` ${phase.dropped === 1 ? "One plant was" : `${phase.dropped} plants were`} deleted while you mapped, so ${phase.dropped === 1 ? "its pin was" : "their pins were"} left out.`;
    return <Message text={`Map saved as version ${phase.version}: ${sessionSummary(state)}.${dropped}`} onClose={actions.close} done />;
  }

  const area = openBedArea(state);
  const hint = session.interrupted ? "The camera is paused. Mapping carries on when it's back" : TRACKING_HINT[session.tracking];

  // A bed still being drawn isn't saved silently: close it first, or leave it out (contracts 9 takes closed beds).
  const confirmSave = () => {
    const open = openBed(state);
    if (open === "none") return actions.save();
    if (open === "closable") {
      Alert.alert("This bed isn't closed", "Close it and save it with the map, or leave it out.", [
        { text: "Cancel", style: "cancel" },
        { text: "Leave it out", style: "destructive", onPress: () => actions.save() },
        { text: "Close and save", onPress: () => actions.save(`Bed ${state.beds.length + 1}`) },
      ]);
      return;
    }
    Alert.alert("This bed has too few corners", "A bed needs at least 3 corners, so this one is left out.", [
      { text: "Cancel", style: "cancel" },
      { text: "Save without it", style: "destructive", onPress: () => actions.save() },
    ]);
  };

  return (
    <View style={{ flex: 1, backgroundColor: camera.black }}>
      <StatusBar style="light" />
      <GardenMappingView
        ref={view}
        tapMode={tap}
        style={{ position: "absolute", top: 0, left: 0, right: 0, bottom: 0 }}
        onTrackingState={(e) => actions.onTracking(e.nativeEvent.state)}
        onGroundTap={(e) => actions.dispatch({ type: "corner", at: [e.nativeEvent.x_m, e.nativeEvent.y_m] })}
        onPinPlaced={(e) => actions.dispatch({ type: "pin", at: [e.nativeEvent.x_m, e.nativeEvent.y_m], canopy: e.nativeEvent.canopy_radius_m })}
        onHeading={(e) => actions.dispatch({ type: "heading", deg: e.nativeEvent.north_deg, accuracy: e.nativeEvent.accuracy_deg })}
        onInterruption={(e) => actions.onInterruption(e.nativeEvent.interrupted)}
        onMountError={(e) => setMountError(e.nativeEvent.message || "The camera couldn't start for mapping.")}
      />

      <View style={{ position: "absolute", top: insets.top + 8, left: 12, right: 12, flexDirection: "row", alignItems: "center", justifyContent: "space-between", gap: space[3] }}>
        <Button label="Close" variant="secondary" size="small" onPress={actions.close} />
        <View style={{ paddingHorizontal: 12, paddingVertical: 6, borderRadius: 999, backgroundColor: camera.pill, flexShrink: 1 }}>
          <Text variant="footnote" style={{ color: camera.ink }} numberOfLines={1} testID="mapping-summary">
            {hint ?? sessionSummary(state)}
          </Text>
        </View>
        <Button
          label="Save"
          size="small"
          busy={session.saving}
          disabled={!session.canSave}
          onPress={confirmSave}
        />
      </View>

      <View style={{ position: "absolute", left: 12, right: 12, bottom: insets.bottom + 12, padding: space[4], gap: space[3], borderRadius: 28, backgroundColor: camera.card }}>
        {phase.type === "choose" ? (
          <View style={{ gap: space[3] }}>
            <Text variant="headline" style={{ color: camera.ink }}>
              This phone mapped the current version.
            </Text>
            <Text variant="subheadline" style={{ color: camera.inkMuted }}>
              Edit finds it again where you stand and keeps its beds and pins. Map again starts from nothing.
            </Text>
            <View style={{ flexDirection: "row", gap: space[3] }}>
              <Button label="Edit the map" onPress={actions.edit} style={{ flex: 1 }} />
              <Button label="Map again" variant="secondary" onPress={actions.mapAgain} style={{ flex: 1 }} />
            </View>
          </View>
        ) : phase.type === "relocalising" ? (
          <View style={{ gap: space[3] }}>
            <View style={{ flexDirection: "row", gap: space[3], alignItems: "center" }}>
              <ActivityIndicator color={camera.ink} />
              <Text variant="subheadline" style={{ color: camera.ink, flex: 1 }}>
                Finding the saved map. Point the phone at the beds you mapped last time.
              </Text>
            </View>
            <Button label="Map again instead" variant="secondary" onPress={actions.mapAgain} />
          </View>
        ) : phase.type === "stale" ? (
          <View style={{ gap: space[3] }}>
            <Text variant="headline" style={{ color: camera.ink }}>
              Someone mapped the garden since you started.
            </Text>
            <Text variant="subheadline" style={{ color: camera.inkMuted }}>
              Their map is version {phase.current}. Replace it with what you mapped here, or keep theirs. Mapping again starts
              from nothing, and saving that replaces their map too.
            </Text>
            <View style={{ flexDirection: "row", gap: space[3] }}>
              <Button label="Replace theirs" onPress={() => actions.replace(phase.current)} style={{ flex: 1 }} />
              <Button label="Keep theirs" variant="secondary" onPress={actions.close} style={{ flex: 1 }} />
            </View>
            <Button label="Map again" variant="plain" onPress={() => actions.mapAgainFromTheirs(phase.current)} />
          </View>
        ) : (
          <>
            <View style={{ flexDirection: "row", gap: space[2] }} accessibilityRole="tablist">
              {(["corner", "pin"] as const).map((t) => (
                <Pressable
                  key={t}
                  accessibilityRole="tab"
                  accessibilityState={{ selected: tap === t }}
                  onPress={() => setTap(t)}
                  style={{ flex: 1, minHeight: minTouch, borderRadius: 999, alignItems: "center", justifyContent: "center", backgroundColor: tap === t ? camera.selected : camera.pill }}
                >
                  <Text variant="headline" style={{ color: camera.ink }}>
                    {t === "corner" ? "Beds" : "Plants"}
                  </Text>
                </Pressable>
              ))}
            </View>
            {tap === "corner" ? (
              <BedControls
                corners={state.corners.length}
                area={area}
                onClose={() => promptName("Name this bed", `Bed ${state.beds.length + 1}`, (name) => actions.dispatch({ type: "closeBed", name }))}
              />
            ) : (
              <PlantControls session={session} />
            )}
            {session.problem ? (
              <Pressable onPress={actions.dismissProblem} accessibilityRole="alert" accessibilityHint="Dismisses the message">
                <Text variant="subheadline" style={{ color: camera.ink }}>
                  {session.problem}
                </Text>
              </Pressable>
            ) : null}
            <View style={{ flexDirection: "row", justifyContent: "space-between" }}>
              <Button label="Undo" variant="plain" size="small" disabled={state.history.length === 0} onPress={() => actions.dispatch({ type: "undo" })} />
              <Button label="Start again" variant="plain" size="small" disabled={state.history.length === 0} onPress={actions.startAgain} />
            </View>
          </>
        )}
      </View>
    </View>
  );
}

function BedControls({ corners, area, onClose }: { corners: number; area?: string; onClose: () => void }) {
  return (
    <View style={{ gap: space[2] }}>
      <Text variant="subheadline" style={{ color: camera.ink }} testID="bed-instruction">
        {corners === 0
          ? "Tap the ground at each corner of a bed, going round it."
          : `${corners} ${corners === 1 ? "corner" : "corners"}${area ? ` · ${area}` : ""}. Tap the next corner, or close the bed.`}
      </Text>
      <Button label="Close the bed" variant="secondary" disabled={corners < 3} onPress={onClose} />
    </View>
  );
}

function PlantControls({ session }: { session: ReturnType<typeof useMappingSession> }) {
  const next = session.state.next;
  const nextName = next ? ("plantId" in next ? next.name : next.newName) : undefined;
  return (
    <View style={{ gap: space[2] }}>
      <Text variant="subheadline" style={{ color: camera.ink }}>
        {nextName ? `Tap ${nextName} to drop its pin.` : "Choose a plant, then tap it."}
      </Text>
      <ScrollView horizontal showsHorizontalScrollIndicator={false} contentContainerStyle={{ gap: space[2] }}>
        {session.plants.map((p) => {
          const selected = next !== undefined && "plantId" in next && next.plantId === p.id;
          const done = session.pinned.has(p.id);
          return (
            <Pressable
              key={p.id}
              accessibilityRole="button"
              accessibilityState={{ selected }}
              accessibilityHint={done ? "Pinned. Choosing it again moves its pin." : undefined}
              onPress={() => session.actions.choosePlant({ plantId: p.id, name: p.name })}
              style={{ minHeight: minTouch, paddingHorizontal: 14, borderRadius: 999, justifyContent: "center", backgroundColor: selected ? camera.ink : camera.pill }}
            >
              <Text variant="subheadline" style={{ color: selected ? camera.onInk : camera.ink }}>
                {done ? `${p.name} ✓` : p.name}
              </Text>
            </Pressable>
          );
        })}
        <Pressable
          accessibilityRole="button"
          onPress={() => promptName("A new plant", "", (name) => name.trim() && session.actions.choosePlant({ newName: name }))}
          style={{ minHeight: minTouch, paddingHorizontal: 14, borderRadius: 999, justifyContent: "center", borderWidth: 1, borderColor: camera.ink }}
        >
          <Text variant="subheadline" style={{ color: camera.ink }}>
            New plant
          </Text>
        </Pressable>
      </ScrollView>
    </View>
  );
}

/** The platform's own text prompt (iOS), so naming a bed or plant doesn't cover the camera with a sheet. */
function promptName(title: string, placeholder: string, onDone: (name: string) => void) {
  Alert.prompt(title, undefined, [{ text: "Cancel", style: "cancel" }, { text: "Done", onPress: (value?: string) => onDone(value ?? placeholder) }], "plain-text", placeholder);
}

function Message({
  text,
  busy,
  done,
  retry,
  retrying,
  onClose,
}: {
  text: string;
  busy?: boolean;
  done?: boolean;
  retry?: () => void;
  retrying?: boolean;
  onClose: () => void;
}) {
  return (
    <View style={{ flex: 1, backgroundColor: camera.black, justifyContent: "center", padding: space[6], gap: space[4] }}>
      <StatusBar style="light" />
      {busy ? <ActivityIndicator color={camera.ink} /> : null}
      {text ? (
        <Text variant="title3" style={{ color: camera.ink }}>
          {text}
        </Text>
      ) : null}
      {retry ? <Button label="Try again" onPress={retry} busy={retrying} /> : null}
      <Button label={done ? "Done" : "Close"} variant={done ? "primary" : "secondary"} onPress={onClose} />
    </View>
  );
}
