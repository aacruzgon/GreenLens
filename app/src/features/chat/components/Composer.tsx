// The composer (screens 3.6): up to 3 photos and 2,000 characters. A photo that fails its upload checks shows the
// reason under its thumbnail with Retry and Remove, and the message can be sent without it (flows 9).
import { Image } from "expo-image";
import { ActivityIndicator, Pressable, TextInput, View } from "react-native";

import { LineIcon, Text, minTouch, radii, space, useTheme } from "@/ui";

import { MAX_PHOTOS, MAX_TEXT, type ComposerPhoto } from "../hooks/useConversationScreen";

export interface ComposerProps {
  text: string;
  onChangeText: (t: string) => void;
  photos: readonly ComposerPhoto[];
  canSend: boolean;
  busy: boolean;
  onSend: () => void;
  onAddPhoto: () => void;
  onRetryPhoto: (localId: string) => void;
  onRemovePhoto: (localId: string) => void;
  bottomInset: number;
}

function Thumb({ photo, onRetry, onRemove }: { photo: ComposerPhoto; onRetry: () => void; onRemove: () => void }) {
  const { colors } = useTheme();
  return (
    <View style={{ width: 76, gap: 4 }} testID={`composer-photo-${photo.status}`}>
      <View style={{ width: 76, height: 76, borderRadius: radii.thumb, overflow: "hidden", backgroundColor: colors.wash }}>
        <Image source={{ uri: photo.uri }} contentFit="cover" style={{ flex: 1, opacity: photo.status === "ready" ? 1 : 0.5 }} accessibilityLabel="Attached photo" />
        {photo.status === "uploading" ? <ActivityIndicator style={{ position: "absolute", top: 28, left: 28 }} color={colors.text} /> : null}
        {/* A 44 pt target in the corner, around the small visible circle. */}
        <Pressable
          accessibilityRole="button"
          accessibilityLabel="Remove photo"
          onPress={onRemove}
          testID="composer-photo-remove"
          style={{ position: "absolute", top: 0, right: 0, width: minTouch, height: minTouch, alignItems: "flex-end", padding: 4 }}
        >
          <View style={{ width: 22, height: 22, borderRadius: 11, alignItems: "center", justifyContent: "center", backgroundColor: colors.surface }}>
            <LineIcon name="x" color={colors.text} size={12} />
          </View>
        </Pressable>
      </View>
      {photo.status === "error" ? (
        <>
          <Text variant="caption2" color="danger" numberOfLines={3}>
            {photo.error}
          </Text>
          <Pressable accessibilityRole="button" accessibilityLabel="Retry the upload" onPress={onRetry} style={{ minHeight: minTouch, justifyContent: "center" }}>
            <Text variant="caption1" color="accent" emphasis>
              Retry
            </Text>
          </Pressable>
        </>
      ) : null}
    </View>
  );
}

export function Composer({ text, onChangeText, photos, canSend, busy, onSend, onAddPhoto, onRetryPhoto, onRemovePhoto, bottomInset }: ComposerProps) {
  const { colors } = useTheme();
  const over = text.trim().length > MAX_TEXT;
  return (
    <View style={{ borderTopWidth: 1, borderTopColor: colors.border, backgroundColor: colors.background, paddingHorizontal: space[3], paddingTop: space[2], paddingBottom: Math.max(bottomInset, space[2]), gap: space[2] }}>
      {photos.length ? (
        <View style={{ flexDirection: "row", gap: space[2] }}>
          {photos.map((p) => (
            <Thumb key={p.localId} photo={p} onRetry={() => onRetryPhoto(p.localId)} onRemove={() => onRemovePhoto(p.localId)} />
          ))}
        </View>
      ) : null}
      <View style={{ flexDirection: "row", alignItems: "flex-end", gap: space[2] }}>
        <Pressable
          accessibilityRole="button"
          accessibilityLabel="Attach a photo"
          accessibilityHint={`Up to ${MAX_PHOTOS} photos`}
          disabled={photos.length >= MAX_PHOTOS}
          onPress={onAddPhoto}
          testID="composer-photo"
          style={({ pressed }) => ({ width: minTouch, height: minTouch, alignItems: "center", justifyContent: "center", opacity: photos.length >= MAX_PHOTOS ? 0.35 : pressed ? 0.6 : 1 })}
        >
          <LineIcon name="camera" color={colors.text} size={22} />
        </Pressable>
        <TextInput
          value={text}
          onChangeText={onChangeText}
          placeholder="Ask about your plants…"
          placeholderTextColor={colors.textMuted}
          multiline
          accessibilityLabel="Message"
          testID="composer-input"
          style={{ flex: 1, minHeight: minTouch, maxHeight: 140, paddingHorizontal: space[3], paddingTop: 11, paddingBottom: 11, borderRadius: 22, backgroundColor: colors.surfaceMuted, color: colors.text, fontSize: 17, lineHeight: 22 }}
        />
        <Pressable
          accessibilityRole="button"
          accessibilityLabel="Send"
          accessibilityState={{ disabled: !canSend, busy }}
          disabled={!canSend}
          onPress={onSend}
          testID="composer-send"
          style={({ pressed }) => ({ width: minTouch, height: minTouch, borderRadius: minTouch / 2, alignItems: "center", justifyContent: "center", backgroundColor: canSend ? colors.accent : colors.surfaceMuted, opacity: pressed ? 0.7 : 1 })}
        >
          <LineIcon name="send" color={canSend ? colors.accentText : colors.textMuted} size={20} strokeWidth={2} />
        </Pressable>
      </View>
      {over ? (
        <Text variant="caption1" color="danger" accessibilityLiveRegion="polite">
          {text.trim().length - MAX_TEXT} characters over the limit of {MAX_TEXT}.
        </Text>
      ) : null}
    </View>
  );
}
