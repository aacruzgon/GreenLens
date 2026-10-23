import { useActiveGardenId } from "@/features/gardens";
import { TasksScreen } from "@/features/tasks";

export default function TasksTab() {
  return <TasksScreen gardenId={useActiveGardenId()} />;
}
