import { ConversationListScreen } from "@/features/chat";
import { useActiveGardenId } from "@/features/gardens";

export default function ChatTab() {
  return <ConversationListScreen gardenId={useActiveGardenId()} />;
}
