// The Tasks tab (roadmap 10 item 9; screens 3.7): Open, Done and the alert inbox, grouped by due date in the
// garden's time zone. Editors and owners swipe a task for Done, Dismiss (plan and alert tasks) or Delete (their own
// and chat's), and the same actions are VoiceOver actions on the row. Viewers see the lists read-only.
import { FlashList } from "@shopify/flash-list";
import { Pressable, RefreshControl, View } from "react-native";
import ReanimatedSwipeable from "react-native-gesture-handler/ReanimatedSwipeable";
import { useSafeAreaInsets } from "react-native-safe-area-context";

import { Banner, Button, Chip, EmptyState, ErrorState, LineIcon, Skeleton, Text, Wash, gutter, minTouch, space, useTheme, type ColorRole } from "@/ui";

import { TaskRow } from "../components/TaskRow";
import { useTasksTab, type TaskRowModel } from "../hooks/useTasksTab";
import type { TaskView } from "../model/groups";

const VIEWS: { value: TaskView; label: string }[] = [
  { value: "open", label: "Open" },
  { value: "done", label: "Done" },
  { value: "alerts", label: "Alerts" },
];

const EMPTY: Record<TaskView, { title: string; message: string }> = {
  open: { title: "Nothing to do", message: "Tasks from your plans, the forecast and your own list show here." },
  done: { title: "Nothing done yet", message: "Tasks you finish show here." },
  alerts: { title: "No alerts", message: "Frost, heat and rain alerts for your garden show here." },
};

/** The list's items: a date heading, or a task (FlashList takes one flat list; README 4). */
type Item = { kind: "header"; key: string; title: string } | { kind: "row"; key: string; row: TaskRowModel };

function SwipeAction({ label, fg, bg, onPress }: { label: string; fg: ColorRole; bg: ColorRole; onPress: () => void }) {
  const { colors } = useTheme();
  // Hidden from VoiceOver: the row carries the same actions as accessibility actions.
  return (
    <Pressable
      onPress={onPress}
      accessibilityElementsHidden
      importantForAccessibility="no-hide-descendants"
      style={({ pressed }) => ({ minWidth: 84, paddingHorizontal: space[3], alignItems: "center", justifyContent: "center", backgroundColor: colors[bg], opacity: pressed ? 0.7 : 1 })}
    >
      <Text variant="subheadline" emphasis color={fg}>
        {label}
      </Text>
    </Pressable>
  );
}

function Row({ row, view, canWrite, onOpen, onDone, onDismiss, onDelete }: { row: TaskRowModel; view: TaskView; canWrite: boolean; onOpen: () => void; onDone: () => void; onDismiss: () => void; onDelete: () => void }) {
  const { colors } = useTheme();
  const a = row.actions;
  const any = a.done || a.dismiss || a.delete;
  const a11yActions = [
    ...(a.done ? [{ name: "done", label: "Done" }] : []),
    ...(a.dismiss ? [{ name: "dismiss", label: "Dismiss" }] : []),
    ...(a.delete ? [{ name: "delete", label: "Delete" }] : []),
  ];
  const meta = [row.source, row.meta, view === "alerts" && row.dismissed ? "Dismissed" : undefined].filter(Boolean).join(" · ") || undefined;
  const body = (
    <View
      style={{ backgroundColor: colors.background }}
      accessibilityActions={a11yActions}
      onAccessibilityAction={(e) => {
        if (e.nativeEvent.actionName === "done") onDone();
        if (e.nativeEvent.actionName === "dismiss") onDismiss();
        if (e.nativeEvent.actionName === "delete") onDelete();
      }}
      testID={`task-${row.task.id}`}
    >
      <TaskRow
        title={row.title}
        meta={meta}
        done={row.done}
        onCheck={canWrite && !row.dismissed ? onDone : undefined}
        onPress={row.pressHint ? onOpen : undefined}
        pressHint={row.pressHint}
      />
    </View>
  );
  if (!any) return body;
  return (
    <ReanimatedSwipeable
      friction={2}
      rightThreshold={40}
      renderRightActions={(_p, _t, methods) => (
        <View style={{ flexDirection: "row" }}>
          {a.dismiss ? <SwipeAction label="Dismiss" fg="watch" bg="watchSoft" onPress={() => (methods.close(), onDismiss())} /> : null}
          {a.delete ? <SwipeAction label="Delete" fg="danger" bg="dangerSoft" onPress={() => (methods.close(), onDelete())} /> : null}
          {a.done ? <SwipeAction label="Done" fg="healthy" bg="healthySoft" onPress={() => (methods.close(), onDone())} /> : null}
        </View>
      )}
    >
      {body}
    </ReanimatedSwipeable>
  );
}

export function TasksScreen({ gardenId }: { gardenId: string }) {
  const { colors } = useTheme();
  const insets = useSafeAreaInsets();
  const t = useTasksTab(gardenId);

  const header = (
    <View style={{ paddingTop: insets.top + space[2], paddingHorizontal: gutter, gap: space[3], paddingBottom: space[2] }}>
      <View style={{ flexDirection: "row", alignItems: "center", justifyContent: "space-between" }}>
        <Text variant="largeTitle" emphasis>
          Tasks
        </Text>
        {t.canWrite ? (
          <Button
            label="Task"
            variant="secondary"
            size="small"
            onPress={t.actions.add}
            icon={({ color }) => <LineIcon name="plus" color={color} size={18} />}
            accessibilityLabel="New task"
            testID="task-add"
          />
        ) : null}
      </View>
      <View accessibilityRole="tablist" style={{ flexDirection: "row", gap: space[2] }}>
        {VIEWS.map((v) => (
          <Chip key={v.value} label={v.label} selected={t.view === v.value} onPress={() => t.actions.setView(v.value)} />
        ))}
      </View>
      {t.problem ? <Banner tone="danger" title={t.problem} /> : null}
    </View>
  );

  return (
    <View style={{ flex: 1, backgroundColor: colors.background }} testID="tasks-screen">
      <Wash />
      {t.error ? (
        <>
          {header}
          <ErrorState message={t.error} onRetry={t.actions.retry} />
        </>
      ) : t.loading ? (
        <>
          {header}
          <View style={{ paddingHorizontal: gutter, gap: space[3] }}>
            {[0, 1, 2, 3].map((i) => (
              <Skeleton key={i} height={minTouch} />
            ))}
          </View>
        </>
      ) : (
        <FlashList
          data={t.sections.flatMap((sec): Item[] => [{ kind: "header", key: `h-${sec.key}`, title: sec.title }, ...sec.rows.map((row) => ({ kind: "row" as const, key: row.task.id, row }))])}
          keyExtractor={(item) => item.key}
          getItemType={(item) => item.kind}
          ListHeaderComponent={header}
          refreshControl={<RefreshControl refreshing={t.refreshing} onRefresh={t.actions.refresh} />}
          contentContainerStyle={{ paddingBottom: insets.bottom + space[16] }}
          renderItem={({ item }) =>
            item.kind === "header" ? (
              <Text variant="footnote" color="textMuted" emphasis accessibilityRole="header" style={{ paddingHorizontal: gutter, paddingTop: space[4], paddingBottom: space[1], textTransform: "uppercase", letterSpacing: 0.6 }}>
                {item.title}
              </Text>
            ) : (
              <Row
                row={item.row}
                view={t.view}
                canWrite={t.canWrite}
                onOpen={() => t.actions.open(item.row.task)}
                onDone={() => t.actions.done(item.row.task)}
                onDismiss={() => t.actions.dismiss(item.row.task)}
                onDelete={() => t.actions.delete(item.row.task)}
              />
            )
          }
          ListEmptyComponent={
            <EmptyState
              title={EMPTY[t.view].title}
              message={EMPTY[t.view].message}
              action={t.view === "open" && t.canWrite ? { label: "New task", onPress: t.actions.add, variant: "secondary" } : undefined}
            />
          }
        />
      )}
    </View>
  );
}
