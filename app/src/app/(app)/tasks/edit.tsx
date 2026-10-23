import { useActiveGardenId } from "@/features/gardens";
import { TaskEditorSheet } from "@/features/tasks";

export default function TaskEditor() {
  return <TaskEditorSheet gardenId={useActiveGardenId()} />;
}
