import { CardGroup } from "./ui";

const MENU_ITEMS: { title: string; description: string }[] = [
  {
    title: "新建文件",
    description: "文本文件 (.txt)、Markdown (.md)",
  },
  {
    title: "在终端中打开",
    description: "在新 Terminal 窗口中 cd 到所选文件夹",
  },
];

export default function ContextMenuSettings() {
  return (
    <div className="space-y-[16px]">
      <CardGroup title="Finder 右键菜单">
        <p className="text-[13px]/[18px] text-white/60">
          Finder 和桌面右键直接出现下面的动作；授权与启停见「通用 → 权限与授权」。
        </p>
        <ul className="mt-[12px] space-y-[8px]">
          {MENU_ITEMS.map((item) => (
            <li key={item.title} className="flex items-start gap-[8px]">
              <span className="mt-[6px] h-[6px] w-[6px] shrink-0 rounded-full bg-white/25" />
              <span className="min-w-0">
                <span className="text-[13px]/[18px] text-white">{item.title}</span>
                <span className="ml-[8px] text-[12px]/[16px] text-white/45">
                  {item.description}
                </span>
              </span>
            </li>
          ))}
        </ul>
      </CardGroup>
    </div>
  );
}
