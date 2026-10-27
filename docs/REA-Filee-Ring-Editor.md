# Filee 设置页圆环编辑器逆向规范

> **状态：历史逆向记录，已被 `docs/REA-Filee-Ring-Integrated.md` 取代（2026-10-27）。**
> 设置页已不再有「正/背面切换」：现在是**文件类型 Profile 列表 + 当前 Profile 的单面圆环**。
> 仍然有效的部分：扇区拖入/排序/移除的交互约束、可用预设列表的筛选与编辑入口。

> 范围：仅设置页中的圆环编排区与“可用预设”列表，包括正/背面切换、拖入、排序、移除、分组入口、搜索筛选和编辑入口。
>
> 不包含：Option 全局唤起、文件管理器识别、文件拖入转换、运行态分组展开、转换队列、进度/成功动画，以及左侧文件类型 Profile 管理页面。
>
> 证据性质：以下来自 Filee 源码静态分析，未在本轮运行 Filee 或 ClipBase 做 UI 验证。ClipBase 的 AppKit 动画参数和触摸板手感仍需实机校验。
>
> **时效说明（2026-10-26）**：本文写于 ClipBase 还是 AppKit 原生的阶段。现在的环形编辑是
> Tauri webview 里的 React 组件（`src/components/RingEditor.tsx` + 设置页 `src/pages/RingSettings.tsx`），
> 宿主模型是 `RingConfig { presets, actions, back_actions }`。文中「NSView / CASpringAnimation 边界」
> 属于历史设计；可读信息（编排结构、拖入/排序/翻面语义、数据模型）仍然有效，**不要照 NSView 方案实现**。

## 0. 结论与证据

Filee 的设置页圆环不是另一套简化控件，而是 `DonutMenu` 的 `IsEditMode`。页面负责拖放手势和模型变更，圆环负责绘制、命中、编辑按钮与插入预览；ViewModel 负责正背面数据、候选预设和持久化。

核心源码：

- [ToolbarPage.axaml.cs](/Users/dayiteng/Filee/src/Filee.App/Views/Pages/ToolbarPage.axaml.cs)：编辑器拖放状态、落点、ghost 与清理。
- [ToolbarPage.axaml](/Users/dayiteng/Filee/src/Filee.App/Views/Pages/ToolbarPage.axaml)：正/背面开关、圆环、搜索、分类和预设 chip 的布局绑定。
- [ToolbarPageViewModel.cs](/Users/dayiteng/Filee/src/Filee.App/ViewModels/Pages/ToolbarPageViewModel.cs)：插入、移动、删除、过滤、计数和保存。
- [DonutMenu.cs](/Users/dayiteng/Filee/src/Filee.App/Controls/DonutMenu.cs)：编辑模式、扇区拖动事件、编辑按钮与插入间隙动画。
- [DonutGeometry.cs](/Users/dayiteng/Filee/src/Filee.App/Controls/DonutGeometry.cs)：角度、坐标、命中与 slot 计算。
- [DonutMenu.Flip.cs](/Users/dayiteng/Filee/src/Filee.App/Controls/DonutMenu.Flip.cs)：编辑器换面动画与运行态翻面的隔离。
- [ToolbarProfile.cs](/Users/dayiteng/Filee/src/Filee.Core/Profiles/ToolbarProfile.cs)：每面 12 项及正背面 ID 列表。

## 1. 必须原样对齐的编辑交互（A）

### 1.1 正背面

- 圆环上方是 `Front / Back` 双段开关，并分别显示两面当前数量。
- 当前面决定圆环显示和全部增删排序操作作用于 `PresetIds` 还是 `BackPresetIds`。
- 圆心标题为当前 Profile 名称；正面副标题为 `数量 / 12`，背面额外标明 Back。
- 切面前先终止当前拖动，清除 ghost、插入预览、源扇区淡化及移除高亮，再重建新面并播放入场波。
- 设置页不调用运行态 `Flip()`：`Flip()` 在编辑模式直接返回 `false`；编辑器使用 `PlayFlipIn()`。
- 背面为空仍可编辑和接收首个预设，不因为背面为空禁用切换。

证据：`ToolbarPage.axaml:220-244`、`ToolbarPage.axaml.cs:72-79`、`ToolbarPageViewModel.cs:266-299`、`DonutMenu.Flip.cs:275-278,319-329`。

### 1.2 从候选列表添加

| 操作 | 结果 |
|---|---|
| 左键拖动可用 chip，移动达到 5 px | 开始添加拖动，显示跟随指针的名称 ghost |
| 拖到圆环有效插入 slot | 在该位置插入 preset ID |
| 拖到圆心 | 追加到当前面末尾 |
| 双击 chip | 追加到当前面末尾 |
| chip 聚焦后按 Enter | 追加到当前面末尾 |
| 右键 chip | 打开该 preset 编辑器，不开始添加 |
| Shift+F10 / Menu 键 | 键盘打开 preset 编辑器 |
| 松开在无效区域 | 不变更模型 |

约束：

- 同一面不得重复同一 preset ID。
- 每面最多 12 项；满员时拒绝插入并显示上限提示。
- 候选列表排除正背面已放置 ID 的并集，因此正常 UI 流程不会把同一 preset 再放到另一面。
- `CanAdd` 在 chip 拖动开始时固定；拖动中搜索或分类刷新不能改变这次拖动是否可添加。
- 不可用于当前 Profile 的 preset 可以因搜索而显示，以便查看原因和编辑，但不能添加。

证据：`ToolbarPage.axaml.cs:102-145,166-189`、`ToolbarPageViewModel.cs:58-63,409-425,600-649`。

### 1.3 圆环内排序

- 在扇区按下并移动超过 5 px 后进入排序拖动。
- 原扇区淡化；其他扇区为预览位置弹开，目标位置显示半透明、带拖动名称的临时扇区。
- 松开在有效圆环 slot：先移除原 ID，再将 ID 插入目标位置。
- `from == to` 不变更。
- 已有扇区拖到圆心不表示追加，也不执行排序。
- 松开在圆环和候选列表之外：取消，不改变顺序。

证据：`DonutMenu.cs:439-443`、`ToolbarPage.axaml.cs:166-189,197-213`、`ToolbarPageViewModel.cs:435-444`。

### 1.4 移除

- 将已有扇区拖到整个候选预设面板区域，而不只是某个 chip 上。
- 拖动经过候选面板时：关闭圆环插入预览，候选面板显示危险/移除高亮。
- 在候选面板松开：从当前面移除该 ID，正背面计数与候选列表立即刷新。
- 从候选列表发起的添加拖动经过候选列表自身，不进入移除状态。

证据：`ToolbarPage.axaml.cs:157-164,174-187`、`ToolbarPage.axaml:24-27,414-437`。

### 1.5 编辑入口

- hover 扇区显示铅笔按钮；点击铅笔或右键扇区，打开对应 preset 编辑器。
- 编辑按钮优先于普通扇区命中，命中半径为 13 px。
- preset 保存后，圆环标签、caption、候选可用性和相关计数刷新。
- 分组在候选列表中也是一种可拖入的 chip；本专项不展开其运行态子圆环，也不把分组成员管理塞进圆环 NSView。

证据：`DonutMenu.cs:155-156,193-203,364-371`、`ToolbarPage.axaml.cs:52-59`、`ToolbarPageViewModel.cs:446-470,472-491`。

### 1.6 候选列表、搜索与分类

- 操作入口：`New preset`、`New group`。
- 搜索匹配 preset 的显示名称或 caption，忽略大小写；分组同样参与搜索。
- 分类包括 All、Image、Document、Media、Edit、3D/CAD、Map、Archive、Font；只显示当前候选中实际存在的分类。
- 分组始终排在普通转换 preset 前，并在任何分类中保留显示。
- 只有一个分类且它与 All 内容重复时隐藏分类行，避免无意义筛选。
- 切换 Profile 时清空 preset 搜索；新建分组后也清空搜索，让新分组立即可见并可拖动。
- 新 preset 默认尝试追加到当前面；若当前面已满，则保留在候选列表并尽量选中/定位它。
- 可用性判断按当前 Profile 的扩展名上下文进行；未判断完成的普通 preset 暂不进入列表，判断分批执行，避免阻塞 UI。ClipBase 可重写调度方式，但不得把“不可用”误当成“可添加”。

证据：`ToolbarPageViewModel.cs:301-318,472-505,589-746`、`ToolbarPage.axaml:373-443`。

### 1.7 提交时机

每次成功的 Insert、Move、RemoveAt 和 preset 编辑都立即提交一次 Library 保存，然后统一重建圆环、候选列表和计数。没有额外“应用”按钮，也不应等圆环动画结束才保存。

证据：`ToolbarPageViewModel.cs:413-444,464-470,580-587`。

## 2. 编辑器状态机

```text
Idle
├─ ChipPressed(canAdd = true)
│  └─ 达到拖动阈值 → DraggingChip
├─ SlicePressed
│  └─ 超过拖动阈值 → DraggingSlice
├─ FrontBackChanged
│  ├─ CancelDragAndPreview
│  ├─ ReloadCurrentFace
│  └─ PlayIncomingWave
└─ EditSlice / EditChip → HostPresetEditor

DraggingChip
├─ PointerOverRing → ShowInsertPreview(slot)
├─ PointerOverCenter → ShowAppendPreview
├─ DropOnValidSlot → Insert(presetID, slot)
├─ DropOnCenter → Insert(presetID, count)
└─ DropElsewhere / CaptureCancelled → Idle（不变更）

DraggingSlice
├─ PointerOverRing → ShowReorderPreview(slot, sourceIndex)
├─ PointerOverPalette → HideRingPreview + ShowRemoveHighlight
├─ DropOnPalette → RemoveAt(sourceIndex)
├─ DropOnValidSlot → Move(sourceIndex, slot)
└─ DropElsewhere / CaptureCancelled → Idle（不变更）

任意成功变更
└─ Commit → Save → RebuildRingAndPalette → Idle
```

每次拖动至少需要保存：

| 状态 | 含义 |
|---|---|
| `source` | `.chip` 或 `.slice` |
| `presetID` | chip 来源的 preset；slice 来源可为空 |
| `sourceIndex` | slice 原索引；chip 为 -1 |
| `label` | ghost 与预览扇区文字 |
| `startPoint` | 拖动阈值计算 |
| `canAdd` | 拖动开始时的添加资格快照 |
| `isActive` | 是否已越过阈值 |
| `previewSlot` | 当前插入位置，-1 表示无 |
| `isRemoveTargeted` | 当前是否位于移除区域 |

所有退出路径都必须清理临时 UI，包括正常 drop、无效 drop、换面、鼠标捕获丢失和视图销毁。

## 3. 布局与命中数学（A）

### 3.1 基本几何

圆环按当前项数等分，索引 0 从 12 点开始，顺时针排列：

```text
span(n) = n <= 0 ? 2π : 2π / n
mid(i,n) = i × span(n)

x = centerX + radius × sin(angle)
y = centerY - radius × cos(angle)
```

指针角度：

```text
angle = atan2(pointerX - centerX, centerY - pointerY)
若 angle < 0，则 angle += 2π
```

径向命中：

```text
distance < innerRadius              → center
distance > outerRadius + hoverSlack → outside
innerRadius...outerRadius           → ring / emptyRing
```

扇区索引使用扇区中心边界，而不是起点边界：

```text
index = floor((angle + span / 2) / span) mod count
```

视觉圆角、3 px 扇区间缝和 hover 外扩不改变这个原始径向/角度归属；不要改成只命中精确圆角 Path，否则缝隙会出现与 Filee 不同的“死区”。

证据：`DonutGeometry.cs:47-70,115-124`、`DonutMenu.cs:93-97,307-312`。

### 3.2 插入预览几何

预览不是额外画一条线，而是临时改变整个圆环的 slot 布局：

- 添加：`slotCount = count + 1`。
- 排序：先排除源项，再加入预览，因此 `slotCount = count`。
- 源项之前的其他项保持原序号；源项之后先减 1；位于插入点及其后的项再向后移 1。
- 预览扇区从 `span = 0` 弹到 `span(slotCount)`，其他项的 `mid/span` 同步过渡到新目标。
- 添加时，命中测试也使用 `count + 1` 个 slot；这样间隙打开后不会从指针下跳走。
- drop 后模型直接采用预览顺序，避免先回弹再跳到新顺序。

Filee 的弹簧参数为刚度 420、阻尼 36；NSView 可用 `CASpringAnimation` 或逐帧弹簧重实现，但应保留“扇区让位 + 半透明预览”的信息结构。

证据：`DonutMenu.cs:287-351`、`ToolbarPage.axaml.cs:197-213`。

### 3.3 空环与边界

- 空环仍保留完整圆形 drop 区域，可接收第一个 chip。
- 空环的圆心 drop 等价于追加到索引 0。
- 插入索引统一夹到 `0...count`。
- 排序目标由页面限制在有效圆环范围内；ViewModel 再夹到移除后的 `0...count`。
- 12 项已满时仍可排序和移除，只有添加被拒绝。
- 减少动态效果开启时，数据与命中逻辑不变，所有预览直接到目标位置，换面不播放入场波。

## 4. 数据结构与 ClipBase NSView 边界

### 4.1 最小模型

```text
RingProfile
- id: String
- name: String
- frontPresetIDs: [String]       // 顺时针顺序，最多 12
- backPresetIDs: [String]        // 顺时针顺序，最多 12

RingPreset
- id: String
- title: String
- caption: String?
- isGroup: Bool
- category: Enum
- usability: usable | limited(reason) | unusable(reason)

RingSlice
- presetID: String
- title: String
- caption: String?
```

存储格式可直接适配 ClipBase 现有设置体系，不要求复制 Filee 的 JSON 文件或 C# 类型。圆环只持有当前展示快照，不拥有全局 preset 库。

### 4.2 推荐组件拆分

```text
ClipBaseSettingsHost
├─ RingEditorView : NSView
│  ├─ 正/背面切换（也可由宿主放在视图上方）
│  ├─ 圆环绘制与命中
│  ├─ hover 编辑按钮
│  ├─ slice 排序/移除拖动
│  └─ chip 拖入的落点与预览
└─ PresetPaletteView（宿主现有设置页）
   ├─ New preset / New group
   ├─ 搜索和分类
   └─ chip 添加拖动、双击与键盘入口
```

`RingEditorView` 可独立嵌入，不应自行弹独立窗口、读取转换引擎、管理文件类型或写磁盘。宿主负责把 palette drag 与 ring drag 统一到同一个编辑会话；如果拆成两个 NSView，拖动会话协调器属于宿主，而不是让两个视图互相引用。

### 4.3 宿主回调

```text
RingEditorDelegate
- ringEditorDidSelectFace(front/back)
- ringEditorRequestsInsert(presetID, face, index) -> Bool
- ringEditorRequestsMove(face, from, to)
- ringEditorRequestsRemove(face, index)
- ringEditorRequestsEditPreset(presetID)
- ringEditorDragDidChange(previewState)
- ringEditorDragDidEnd()
```

宿主在回调中验证容量、重复 ID 和索引，提交存储后返回新的 `RingProfile` / `RingPreset` 快照。视图不直接把拖放事件当成已保存事实。

### 4.4 当前 ClipBase 接入约束

- 原生侧可挂在 AppKit 设置页，但现有 `ConfigStore` 是应用总配置，不是现成的正背面 preset 库，需要新增宿主模型适配。
- Rust/Tauri 转换命令可提供 preset 能力与可用性信息，但不应进入 NSView 的绘制或命中层。
- 深浅色颜色应映射 ClipBase 主题 token；不要复制 Avalonia Brush 名称。
- AppKit 的拖拽必须同时处理视图内排序与跨视图 palette 添加/移除；仅靠单个 NSView 的本地 `NSDraggingDestination` 不足以覆盖 Filee 的全部手势。

## 5. 可适配 ClipBase 的部分（B）

以下不属于必须逐像素复刻：

- 设置页三列布局、左侧 Profile 卡片、尺寸/中心孔/透明度设置卡片。
- 配色、字体、圆角、阴影、chip 高度、面板宽度和按钮样式。
- 正/背面开关是放在 NSView 内还是由宿主工具栏提供。
- ghost 的具体形状；保留名称跟随指针即可。Filee 的 ghost 偏移为 `(+12,+12)`，可作为初始值。
- 入场波和弹簧的具体动画实现；需保留状态顺序、预览含义和减少动态效果支持。
- 分类标签文案和分类集合，可按 ClipBase 实际转换能力裁剪。
- preset 可用性判断的批处理调度与存储格式。

## 6. 实施验收清单

### 必须通过

- [ ] 正背面各自维护顺序和数量，切面带取消当前拖动。
- [ ] chip 可拖到任意 slot，拖中心追加，双击和 Enter 也可追加。
- [ ] slice 可排序，原项淡化，其余项实时让位，预览位置与最终模型一致。
- [ ] slice 拖到整个 palette 区域可移除，并有明确移除高亮。
- [ ] 同一面不能重复，单面最多 12 项，满员仍允许排序/移除。
- [ ] 正背面任一面已使用的 preset 不再作为可添加 chip 出现。
- [ ] 空环可添加首项；无效区域 drop 和捕获取消均不改变数据。
- [ ] hover 铅笔、右键扇区、右键 chip 都能请求编辑对应 preset。
- [ ] 搜索匹配名称/caption，分组优先，分类不会隐藏分组。
- [ ] 所有成功编辑立即提交并刷新；减少动态效果下功能完全一致。

### 需 AppKit 实机验证

- [ ] 跨 NSView 拖动时 ghost、捕获丢失和 drop 收尾没有残留。
- [ ] 触摸板/鼠标快速拖动时 slot 不抖动，preview 与 drop 使用同一套几何。
- [ ] 1、2、11、12 个扇区时标签、铅笔按钮与命中不互相遮挡。
- [ ] 长名称、本地化、Retina 缩放和深浅色下文字与 ghost 可读。
- [ ] VoiceOver/键盘至少能完成添加、排序、移除、换面和编辑；Filee 已提供 chip 键盘入口，但完整扇区键盘排序属于 ClipBase 需要补齐的适配验证，不能宣称源码已覆盖。

## 7. 不应混入本组件的内容

- 不把 Option 触发、全局鼠标 hook、Finder/Explorer 判断写进编辑器。
- 不在设置页圆环上触发真实转换。
- 不加入运行态 Tab/滚轮翻面、点击转换、文件 drop、进度环和成功勾动画。
- 不让 NSView 直接管理 preset 编辑器窗口、转换器优先级或持久化文件。
- 不为了“还原 Filee”复制 Avalonia 页面布局；复刻的是编辑语义、状态清理和预览几何。
