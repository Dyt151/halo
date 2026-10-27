# Filee 环形转换整体逆向：自定义设置 → 单手势运行 → 格式路由

> 本文是环形转换的总规范，整合此前的设置页、运行态圆环和转换三份专项。
>
> 目标产品约束：**运行态只保留 Option＋左键拖动这一种操作**。不使用快捷键唤起、Finder 右键唤起、单击执行、Enter、方向键、Tab、滚轮、长按或仅按 Option。
>
> 设置页仍使用普通鼠标和键盘编辑；“只有一种操作”限制的是文件转换运行态，不是设置界面。
>
> 证据：Filee 与 ClipBase 当前源码静态阅读。未在本轮启动应用、执行测试或验证真实 macOS 拖放时序。

## 0. 总结：需要修的是整条链，不是再补一个页面

Filee 的完整链路是：

```text
Preset 库：每个动作是一份完整转换配方
        ↓ preset ID
ToolbarProfile：某类文件使用哪些 Preset，以及顺序
        ↓ 按本批文件扩展名选择一个 Profile
DonutItem：保留配置顺序，并计算每个扇区是否可用及原因
        ↓ Option＋左键拖动真实文件
Drop / 松手位置命中一个可用 Preset
        ↓ 文件快照 + Preset 快照
ConversionJob → Route → ConversionStep → 安全输出
        ↓
原地进度 → 成功/失败 → 自动收起或系统通知
```

ClipBase 当前已经具备后半段的大部分基础：`ConversionPreset`、引擎边、Dijkstra Router、Job、逐文件结果、取消、输出分配和原子提交。

当前真正不完整的是前半段和它们之间的连接：

1. **没有 Filee 的 `ToolbarProfile` 层**：所有文件共用一张全局正/背面配置，不能在设置里按 PNG、JPG、文本或混选自定义不同圆环。
2. **正背面与单手势约束冲突**：背面依赖 Tab/滚轮或自动换面；禁用其他操作后，它不是一个可独立到达的界面。
3. **配置布局与运行布局不一致**：设置页保存全部 ID，运行时把不可执行项直接过滤掉，扇区会随文件类型移动；Filee 是保留位置并禁用。
4. **当前拖动以 Finder 选中项快照代替真实拖动载荷**：拖动未选中文件时，松手复核可能刷新会话并返回，导致这一次拖动没有执行；单手势产品不能要求再来一次。
5. **仍存在多个入口和多种运行操作**：全局快捷键、Finder Service、单击、键盘、Tab 和滚轮都与“只用 Option＋左拖”冲突。

因此总方案应收敛为：

```text
PresetLibrary + RingProfile（单面）+ 真实文件拖放 + 单一 RingSession + ConversionJob
```

---

## 1. 设置到运行的唯一数据链

### 1.1 三层模型，缺一不可

```text
ConversionPreset
- id
- name
- target: Explicit(format) | SameAsSource
- image options
- output rule
- reencode_same_format
- builtin

RingProfile
- id
- name
- extensions: [extension]
- kind: Normal | MixedFallback
- preset_ids: [preset ID]       // 唯一一面，顺时针，最多 12

RingSlice
- preset_id
- label / caption
- enabled
- disabled_reason
- convert_count / skip_count
```

职责必须分开：

- **Preset** 回答“怎么转、转成什么、参数和输出是什么”。
- **Profile** 回答“什么文件出现时，盘上提供哪些 Preset”。
- **RingSlice** 回答“这批文件下，该位置现在能否执行，为什么”。
- **Job** 回答“这次实际执行了哪些文件、走了什么路由、每个结果是什么”。

圆环和 Profile 只保存 Preset ID，不复制配方。删除 Preset 时同步清理所有 Profile 的引用；启动 Job 时再克隆 Preset，之后编辑配置不影响运行中任务。

### 1.2 为什么只保留一面

Filee 的背面不是“自动第二页”，它由 Tab/滚轮显式切换。ClipBase 当前虽会在正面为空时自动开背面，但这只是补救，不等于提供一致的可达导航。

在用户限定单手势后：

- 不保留运行态背面。
- 不保留 Tab、滚轮、自动正背面切换和翻面提示。
- 设置页也不再编辑 Front/Back，只编辑 Profile 的唯一 `preset_ids`。
- 不引入需要点击才能展开的分组父扇区；所有能执行的目标直接位于主环。
- 每个 Profile 最多 12 个动作，超出者留在 Preset 库，不占隐藏页面。

这是 ClipBase 对产品约束的适配，不冒充 Filee 仍有相同界面。

### 1.3 设置页的信息架构

建议按 Filee 的真实职责整合，而不是继续把“全局圆环编排”和“转换预设”分成互不相干的卡片：

```text
设置 → 环形转换
├─ 手势与权限
│  ├─ Option＋左键拖动（唯一入口）
│  └─ 辅助功能权限状态
├─ 文件类型 Profile
│  ├─ 新建 / 删除 / 排序 / 命名
│  ├─ 普通 Profile：扩展名 chips
│  └─ 混合 Profile：允许的扩展名；空集合 = catch-all
├─ 当前 Profile 的圆环
│  ├─ 拖入、排序、移除
│  └─ 0...12 个 Preset ID
└─ 可用 Preset / Preset 编辑器
   ├─ 新建、复制、删除、搜索
   ├─ 目标、质量、透明背景、同格式重编码
   └─ 输出目录、命名、重名、OnlyIfSmaller
```

每次成功编辑立即保存，不需要“应用”按钮。保存顺序必须避免产生临时坏引用：

- 新建并直接上环：先保存 Preset，再保存 Profile ID。
- 删除 Preset：同一次配置提交中删除库记录和所有 Profile 引用。
- 配置保存失败：界面不能以“已保存”的本地副本继续覆盖后续编辑。

---

## 2. 文件类型 Profile 的精确规则

### 2.1 扩展名规范化

Filee 的规则可作为直接参照：

- trim、小写。
- 去掉前导 `*` 和 `.`，去掉末尾 `.`。
- 拒绝空字符串、非法字符、连续 `..`，最长 16。
- 复合扩展名只接受注册表已知的值，例如 `tar.gz`；否则普通文件名只按最后一段匹配。
- 同一 Profile 内拒绝重复扩展名。
- 匹配忽略大小写。

ClipBase 当前格式别名也必须进入同一注册表，而不是在设置、运行探测和 Router 中各写一份：

```text
PNG:  png
JPG:  jpg / jpeg / jpe
WebP: webp
GIF:  gif
BMP:  bmp
TIFF: tif / tiff
MD:   md / markdown / mdown / mkd
HTML: html / htm / xhtml
TXT:  txt / text
```

### 2.2 一批文件只选择一个 Profile

Filee 不是给每个文件弹不同圆环，而是对整批扩展名去重后选择同一个 Profile：

```text
1. 第一个“包含本批全部扩展名”的普通 Profile
2. 第一个“扩展名非空且包含本批全部扩展名”的 MixedFallback
3. 第一个扩展名为空的 MixedFallback（catch-all）
4. 没有空 catch-all 时：第一个 MixedFallback
5. 完全没有 MixedFallback 时：第一个 Profile
```

关键语义：

- 普通 Profile 优先于 MixedFallback。
- 普通 Profile 可以包含多个扩展名，因此也能接管同族混合选择。
- 普通 Profile 扩展名为空，不匹配非空文件集合。
- MixedFallback 扩展名为空才表示 catch-all。
- 普通 Profile 间允许重复扩展名，列表第一个获胜；编辑器必须提示遮蔽关系。
- 至少保留一个 MixedFallback，避免未配置类型没有圆环。

### 2.3 Profile 选择与 Preset 可用性是两个阶段

```text
文件路径
 → 扩展名集合
 → ProfileSelector
 → 得到固定 preset_ids 顺序
 → 对每个文件 DetectFormat
 → 对每个 Preset 执行 Route/Skip 可用性计算
 → 同一位置的 RingSlice 变为 enabled 或 disabled
```

**不能因为某 Preset 对当前文件不可用就从数组删除它。** 删除会改变扇区数量、角度和拖动中的落点，造成设置里看到的位置与运行时不同。

Filee 的做法是保留扇区并显示禁用原因。ClipBase 应对齐这一点：

- 可用：正常显示，可拖放执行。
- 当前批次完全不可用：保留位置，降低不透明度，显示原因。
- 松手在禁用项：不启动 Job，按取消/无效落点结束。
- 至少一个文件可执行：Preset 可用，并显示“转换 N · 跳过 M”；执行时所有输入仍获得结果。

---

## 3. 唯一运行手势：Option＋左键拖动

### 3.1 只保留的入口

```text
Finder 前台
+ Option 已在左键按下时保持
+ 左键按下
+ 位移超过拖动阈值
+ 拖动内容包含文件
= 环形转换会话
```

明确不触发：

- 仅按 Option。
- Option＋单击。
- 长按不动。
- 其他 App 中的 Option＋拖动。
- 非文件拖动、框选、文本拖动。
- 全局快捷键、Finder 右键服务、菜单命令。
- 运行盘上的单击、Enter、方向键、Tab、滚轮。

Option 需要在左键按下时存在；接管后左键松开才是提交/取消边界。不要在拖动中途因为 Option 松开而另造第二个提交动作。

### 3.2 Filee 与 ClipBase 当前实现的本质差异

**Filee：真实文件拖放。**

```text
全局手势识别
 → 先建立不可见拖放接收窗口
 → 文件 DragEnter
 → 取得真实拖动文件
 → 显示圆环
 → DragOver 命中
 → Drop 执行
```

**ClipBase 当前：拦截鼠标并读取 Finder 选中项。**

```text
Option 按下时预取 Finder 选中项
 → 左键拖动超过 8px
 → 吞掉后续 dragged 事件
 → 使用预取 paths 建盘
 → 松手再读 Finder 选中项复核
 → 执行或刷新会话
```

当前方案的优点是不依赖 Webview 原生文件 Drop；但它不是 Filee 的真实载荷模型。尤其是拖动一个原本未选中的文件时，预取可能属于旧选择，松手复核发现新文件后会 `probe_and_ready(...); return`，这次拖动并未提交转换。

**单手势约束下，推荐对齐 Filee：以真实拖动文件为权威数据源。** 这样可删除预取 TTL、选择收缩猜测、松手二次选择和“再操作一次”的隐性要求。

如果暂时保留选中项方案，它只能算过渡适配，必须先通过“未选中单文件、未选中项位于多选集合、多选拖动、快速松手”测试，并保证同一次拖动内完成，不得把刷新后的 Ready 留给第二次操作。

### 3.3 单手势状态机

```text
Idle
 └─ Option + LeftDown（Finder）→ Armed

Armed
 ├─ 未过阈值 LeftUp → Idle                 // Option+单击，无圆环
 ├─ 超过阈值 → AwaitingFiles               // 接收窗不可见
 └─ 离开有效现场 / 会话失效 → Idle

AwaitingFiles
 ├─ 非文件 / 无文件进入 / LeftUp → Idle
 └─ FilesEntered(paths) → Probing

Probing
 ├─ 选择 Profile + 构建 Slice → Ready
 ├─ 没有有效文件 → Empty/Unsupported → 自动或松手关闭
 └─ 读取失败 → FailedHint → 松手关闭

Ready（唯一一面）
 ├─ DragOver 可用扇区 → Hover(slice)
 ├─ DragOver 禁用扇区 → DisabledHint
 ├─ DragOver 中心 → CancelHover
 ├─ DragOver 环外 → NoDrop
 └─ LeftUp
    ├─ 可用扇区 → Committing
    ├─ 禁用扇区 / 中心 / 环外 → Closing → Idle
    └─ 非文件区域 → Closing → Idle

Committing
 └─ 固定 paths + preset 快照 → Converting

Converting
 ├─ Progress → 更新原地进度环
 ├─ 5s 仍运行 → Handoff → 系统通知，Job 继续
 ├─ Completed → Success → Hold → Idle
 ├─ CompletedWithErrors / Failed → Failure + 原因 → Hold → Idle
 └─ Cancelled（应用退出等内部取消）→ Idle/通知
```

不再提供运行态 Esc、点外部、点击中心、键盘取消等第二套操作。中心和环外取消仍属于同一次左拖的松手结果。

### 3.4 提交前必须冻结的内容

松手命中后，不允许配置或 Finder 后续变化污染本次任务：

```text
RingExecutionRequest
- generation
- source paths                 // 本次真实拖动文件快照
- profile_id                   // 诊断/历史
- preset snapshot              // 不是执行时再按名称查全局配置
- pointer hit / preset ID
```

`generation` 用于丢弃上一会话迟到的窗口事件。`Converting` 期间不接收第二次手势。进度、结果和通知都必须带 generation 或 Job ID，避免旧结果覆盖新会话。

---

## 4. 圆环运行规则

### 4.1 布局与命中

保留已经核对过的 Filee 几何：

- 索引 0 从 12 点开始，顺时针。
- `span = 2π / N`，扇区中心角 `mid = i × span`。
- 坐标：`x = cx + radius × sin(angle)`，`y = cy - radius × cos(angle)`。
- 命中使用径向距离＋角度，不用圆角 Path contains。
- 中心、主环、环外三类区域；禁用扇区仍是原索引，不产生其他扇区位移。
- 主环最多 12 项，不使用子环和背面。

### 4.2 高亮与提示

- 每次指针移动都重新命中，不能只依赖浏览器 hover，因为接管拖放时窗口可能不接收普通鼠标事件。
- 可用扇区显示 Copy 语义，中心提示“松手转为〈Preset 名〉”。
- 禁用扇区不显示可执行高亮，中心显示具体原因。
- 中心显示“松手取消”。
- 混选时显示“转换 N · 跳过 M”，不能把 M 个输入静默省略。
- 松手必须按**最后位置再补一次命中**，避免最后一段指针事件丢失。

### 4.3 进度与结束

- 松手后扇区向中心收缩，再显示原地进度环。
- 极快任务也保留最短可感知进度/成功时间。
- 成功：✓，自动保持后收起。
- 部分失败或失败：`!` 和首条具体原因，不能只有计数。
- 有保真警告时，成功反馈也要能暴露警告，不能当完全无损。
- 超过 5 秒自动交接系统通知，转换继续。
- 结束后自动回焦 Finder；不需要用户再点击关闭。

---

## 5. 每种格式如何连接

### 5.1 统一规则

当前 ClipBase 有 9 个格式节点、3 个内置引擎：

| 引擎 | 直接连接 | 成本/模式 | 语义 |
|---|---|---|---|
| Image | 6 种图片两两互连，含 self edge | 普通 10；self 12 | 解码→处理→编码 |
| Text | MD/HTML/TXT 两两互连，不含 self | 5，Normal | 真实文本转换 |
| Document | 所有图片→MD | 20，Terminal | 生成引用源图的 Markdown |
| Document | PNG/JPG/WebP/GIF→HTML | 20，Terminal | base64 内联，自包含 HTML |

普通路由最多 3 步。`Terminal` 只能作为最后一步，不能再接到 TXT，避免把“图片文档”错误当成 OCR 文本。

### 5.2 完整格式连接表

| 源格式 | 到其他图片 | 到同格式 | 到 MD | 到 HTML | 到 TXT |
|---|---|---|---|---|---|
| PNG | 直接到 JPG/WebP/GIF/BMP/TIFF | 仅 Preset 允许重编码 | 直接，引用源 PNG | 直接，内联 PNG | 无路由，需 OCR |
| JPG | 直接到 PNG/WebP/GIF/BMP/TIFF | 仅 Preset 允许重编码 | 直接，引用源 JPG | 直接，内联 JPG | 无路由，需 OCR |
| WebP | 直接到 PNG/JPG/GIF/BMP/TIFF | 仅 Preset 允许重编码 | 直接，引用源 WebP | 直接，内联 WebP | 无路由，需 OCR |
| GIF | 直接到 PNG/JPG/WebP/BMP/TIFF | 仅 Preset 允许；多帧逐帧保留 | 直接，引用源 GIF | 直接，内联 GIF | 无路由，需 OCR |
| BMP | 直接到 PNG/JPG/WebP/GIF/TIFF | 仅 Preset 允许重编码 | 直接，引用源 BMP | 经可内联图片中转，当前最低成本为 BMP→PNG→HTML | 无路由，需 OCR |
| TIFF | 直接到 PNG/JPG/WebP/GIF/BMP | 仅 Preset 允许重编码 | 直接，引用源 TIFF | 经可内联图片中转，当前最低成本为 TIFF→PNG→HTML | 无路由，需 OCR |
| MD | 无路由，需渲染引擎 | 无 self edge | 无 self edge | 直接 | 直接 |
| HTML | 无路由，需渲染引擎 | 无 self edge | 直接 | 无 self edge | 直接 |
| TXT | 无路由，需排版/渲染引擎 | 无 self edge | 直接 | 直接 | 无 self edge |

跨行补充：

- 图片→MD 不是 OCR，也不是把图片嵌入 MD；它引用原图。
- 图片→HTML 在直连时是自包含页面；BMP/TIFF 会先转成可内联图片。
- 文本→图片全部无路由；TXT→MD/HTML 是支持的，旧“TXT 只能作为输出”文案不得恢复。
- 图片→TXT 全部无路由；不能因为存在“图片→MD、MD→TXT”就自动串联，因为图片→MD 是 Terminal。

### 5.3 图像保真边界

- 动画 GIF→GIF：保留帧序列和延迟。
- 动画 GIF→其他静态图片：只保留第一帧，必须有警告。
- GIF→HTML/MD：分别内联或引用原 GIF，不等同于静态重编码。
- EXIF 方向烘焙进像素，输出不保留 EXIF，并有警告。
- ICC 不随当前输出保留，并有警告。
- 透明图→JPG 按 Preset 背景色合成。
- 这些警告必须进入 Job 结果和最终反馈，不能只写日志。

### 5.4 Filee 的扩展方式

Filee 的格式覆盖不是一张静态矩阵：`EngineRegistry` 注册图片、PDF、Office、FFmpeg、OCR、Pandoc、LibreOffice 等引擎，`RoutePlanner` 按当前可用引擎动态选路。

因此新增格式时正确顺序是：

```text
注册 Format/别名
 → 引擎声明真实直接边和模式
 → Router 自动得到路线
 → PresetAvailability 自动反映能力
 → Profile 才允许编排对应 Preset
 → Job 执行同一条路线
```

禁止只给设置页增加扩展名或目标按钮，却没有真实引擎边。

---

## 6. 转换执行与输出必须继续共用的规则

这些当前代码已有基础，整合 Profile 和手势时不得退回旧实现：

1. Ring 只提交文件和 Preset，不自己实现转换矩阵。
2. Job 保存文件与 Preset 快照。
3. 每个输入都有 Done/Skipped/Failed/Cancelled，不允许静默忽略。
4. 同格式是否执行由 `reencode_same_format` 和显式 self edge 共同决定。
5. 设置可用性和实际执行使用同一个 Router。
6. 中间步骤写 Job 工作目录，最后步骤写分配器临时文件。
7. 输出路径预留，源文件永不因 Overwrite 被覆盖。
8. 完整成功后提交；失败/取消清理中间文件和未提交输出。
9. 保留逐文件 route、warning、skip reason 和 error。
10. 文件并行和引擎限流不能改变输出命名与结果对应关系。

---

## 7. ClipBase 当前差距清单

| 层 | 当前源码 | 总规范要求 |
|---|---|---|
| 文件类型自定义 | 没有 `RingProfile`；`RingConfig` 是全局 actions/back_actions | 普通＋Mixed Profile，扩展名决定唯一圆环 |
| 圆环面 | 正背面、Tab、滚轮、自动选初始面 | 每 Profile 只有一面，最多 12 |
| 运行入口 | Option 拖动＋全局快捷键＋Finder Service | 仅 Option＋左拖 |
| 运行操作 | 单击、Enter、方向键、Esc、Tab、滚轮 | 仅拖动命中和左键松开 |
| 设置页文案 | 仍宣传右键、单击、键盘、Esc | 只描述真实单手势 |
| 拖动数据源 | Option 按下预取 Finder 选中项，松手复核 | 推荐真实拖动文件；一次手势内完成 |
| 运行布局 | `build_actions` 删除 convert==0 的 Preset | 固定 Profile 顺序，禁用但保留位置 |
| 禁用原因 | 被过滤后无法在对应扇区显示 | Slice 带 disabled_reason |
| 可用性 | 已使用同一 `preset_availability` | 保留，不另写矩阵 |
| Preset | 已有独立库、复制、质量和输出规则 | 保留并接入所有 Profile |
| Router/Job | 已有路由、结果、取消和安全输出 | 保留，避免重复造执行链 |
| 结果反馈 | 有进度、警告、失败和通知 | 保留，自动结束，无额外关闭操作 |

旧文档中“ClipBase 尚无 Preset/Job/Router”的判断只对应旧代码，不再作为当前差距。

---

## 8. 数据迁移与删除范围

### 8.1 配置迁移

建议目标结构：

```text
ring
- enabled
- profiles: [RingProfile]
- last_action_by_profile

conversion
- presets: [ConversionPreset]
```

迁移旧 `actions/back_actions`：

1. 旧正面保持原顺序，作为初始默认 Profile 的主环。
2. 旧背面追加在后并去重，直到 12 项。
3. 超出 12 项的 Preset 不删除，留在 Preset 库和候选面板，并明确提示迁移结果。
4. 旧 `last_action` 能映射则保留为初始高亮；不能只自动执行。
5. 建立 Images、Text、Mixed 三个初始 Profile，具体 Preset 顺序按真实可用能力生成。
6. 不覆盖用户已经自定义的 Preset 参数和输出目录。

### 8.2 运行代码删除/停用范围

在单手势需求下，应删除或停止注册：

- `hotkeys.ring_convert` 及环形快捷键设置。
- Finder Service 的 `OpenRingConvert` 入口。
- `RingManager::start_from_hotkey`、`start_from_paths` 作为用户入口。
- RingWindow 的 onClick、onKeyDown、wheel 和 Face 状态。
- `back_actions`、flip 动画、canFlip、正背面提示。
- 设置页“右键/单击/方向键/Enter/Esc/Tab/滚轮”等运行说明。

设置页自己的右键编辑、键盘排序可以保留；它们不是文件转换运行入口。

### 8.3 不应删除的共用能力

- Preset 编辑和复制。
- 输出保护、Router、Job、取消令牌。
- 进度和系统通知。
- 设置页键盘可访问性。
- 其他剪贴板功能的快捷键；只移除环形转换的额外入口。

---

## 9. A/B 对齐边界

### A：必须对齐的核心行为

- Profile 由本批全部扩展名选择，普通优先、Mixed 兜底、列表顺序解决竞争。
- Profile 存 Preset ID；Preset 是完整配方；Job 使用快照。
- 文件未进入前不显示圆环；只有真实文件才触发转换。
- Option＋左拖一次完成“唤起→选择→松手执行/取消”。
- 扇区位置由设置决定；不可用项禁用而非删除重排。
- 松手按最终位置命中；禁用项、中心、环外不执行。
- 使用 Copy 语义，普通转换绝不移动或删除源文件。
- 可用性和执行共用 Router；每个输入都有结果。
- 原地进度、成功/失败、保真警告和超时通知。

### B：按 ClipBase 适配

- 移除 Filee 的背面、点击模式和额外快捷键，以满足单手势约束。
- 设置页三列或上下布局、配色、字体、阴影和动画实现。
- 使用 AppKit 拖放、Tauri 原生 drag payload 或其他真实文件载荷技术。
- 文件并行度、通知样式和最短结果展示时长。
- 首版只支持当前 9 种格式，后续通过引擎边扩展。

---

## 10. 整体验收清单

### 10.1 设置与 Profile

- [ ] 新建 Images 自定义 Profile，添加 `jpg/jpeg/png`，只编排该 Profile 的 Preset。
- [ ] 同名扩展在多个 Profile 中时，界面明确哪个先匹配。
- [ ] JPG 与 TXT 使用不同 Profile 和不同扇区顺序。
- [ ] Mixed 图片＋文本按规则进入限定 Mixed 或 catch-all，不为每个文件换盘。
- [ ] 新建、复制、删除 Preset 后，所有 Profile 引用一致，无悬空 ID。
- [ ] 两个 JPG Preset 的质量、背景和输出规则互不影响。
- [ ] 设置里 12 个位置与运行时 12 个位置一致；禁用项不导致其他项移动。

### 10.2 单手势

- [ ] 仅 Option、Option＋单击、非文件拖动均不显示圆环。
- [ ] Finder 中 Option＋左拖真实文件，过一次阈值后只出现一次会话。
- [ ] 拖动原本未选中的单个文件，在同一松手中转换该文件，不使用旧选择。
- [ ] 多选中拖动其中一项，转换范围符合真实 drag payload，不靠选择收缩猜测。
- [ ] 拖到扇区后停止移动直接松手，仍按最后位置执行。
- [ ] 松手在禁用项、中心和环外都不创建 Job、不修改文件。
- [ ] 快捷键、右键服务、单击、Enter、Tab 和滚轮均不能唤起或操作运行盘。
- [ ] 转换中第二次手势不会覆盖 paths、Preset 或 generation。
- [ ] 成功/失败/超时后无需第二次操作即可结束或交接通知。

### 10.3 格式连接

- [ ] 逐一验证 6×6 图片边；同格式仅在 Preset 允许时执行。
- [ ] 逐一验证 MD/HTML/TXT 六条有向边。
- [ ] PNG/JPG/WebP/GIF→HTML 为自包含内联；BMP/TIFF→HTML 为两步路由。
- [ ] 所有图片→MD 都明确是引用源图，不宣称 OCR 或嵌入。
- [ ] 图片→TXT、文本→图片均无伪路由，并显示真实原因。
- [ ] Terminal 边不会继续串成图片→MD→TXT。
- [ ] 设置显示可用、松手执行、Job 实际 route 三者一致。

### 10.4 数据安全与反馈

- [ ] 源文件不变；Overwrite 也不能覆盖源路径。
- [ ] 同名并行输出不碰撞；失败/取消无半成品。
- [ ] 未知、空文件、已是目标、无路由、冲突跳过都有逐文件结果。
- [ ] 动画首帧、EXIF、ICC、透明合成等警告能到达最终反馈。
- [ ] 部分成功不会被显示成全部成功， skipped 不会被隐藏。

---

## 11. 主要源码证据

### Filee

- `src/Filee.Core/Profiles/ToolbarProfile.cs`：Profile 模型、匹配顺序、Mixed/catch-all。
- `src/Filee.Core/Presets/Preset.cs`：完整配方和快照。
- `src/Filee.App/ViewModels/Pages/ToolbarPageViewModel.cs`：Profile/Preset/圆环编辑与立即保存。
- `src/Filee.App/ViewModels/Pages/ExtensionInput.cs`：扩展名规范化。
- `src/Filee.App/ViewModels/Pages/ExtensionTagsViewModel.cs`：普通 Profile 扩展名和竞争提示。
- `src/Filee.App/ViewModels/Pages/MixedExtensionsViewModel.cs`：Mixed 编辑和遮蔽提示。
- `src/Filee.App/ViewModels/RadialViewModel.cs`：Load → ProfileSelector → BuildFace → DonutItem。
- `src/Filee.App/Services/PresetAvailability.cs`：Preset 对当前文件的可用性。
- `src/Filee.App/Services/Triggers/GestureDetector.cs`：拖动手势状态。
- `src/Filee.App/Services/RadialController.cs`：拖放窗口与会话。
- `src/Filee.App/Views/RadialWindow.axaml.cs`：DragEnter/DragOver/Drop 和 Copy 语义。
- `src/Filee.Core/Conversion/RoutePlanner.cs`、`JobQueue.cs`：路由与执行。

### ClipBase 当前代码

- `src-tauri/src/config/settings.rs`：当前全局正背面和 Preset 库。
- `src/pages/RingSettings.tsx`：当前设置与仍存在的多入口文案。
- `src-tauri/src/core/option_tap/mod.rs`：CGEventTap、8px 接管、预取和吞事件。
- `src-tauri/src/core/ring/mod.rs`：多入口、选择复核、Snapshot、execute、结果。
- `src/windows/RingWindow.tsx`：拖动事件以及额外单击/键盘/滚轮/正背面。
- `src-tauri/src/core/convert/preset.rs`：Preset 和输出规则。
- `src-tauri/src/core/convert/format.rs`：9 种格式与别名。
- `src-tauri/src/core/convert/edge.rs`：引擎、边和 Terminal 模式。
- `src-tauri/src/core/convert/engines/*.rs`：当前真实直接边和保真警告。
- `src-tauri/src/core/convert/router.rs`：最多三步的 Dijkstra。
- `src-tauri/src/core/convert/job.rs`：逐文件 Job、取消、进度和可用性。
- `src-tauri/src/core/convert/output.rs`：输出分配和源保护。

本文只形成整体规范，未修改代码。运行时结论需在实现后按第 10 章做 macOS 实机验证。
