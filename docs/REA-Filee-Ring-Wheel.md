# Filee 环形工具栏逆向需求规范（源码证据版）

> **状态：历史逆向记录，已被 `docs/REA-Filee-Ring-Integrated.md` 取代（2026-10-27）。**
> 本文描述的「点击模式 / 正反面 / 滚轮换面 / 分组子环 / 全局快捷键唤起」在 ClipBase
> 中**已全部下线**：现在只有一面环，唯一手势是 Finder 里的 Option＋左键拖动。
> 仍然有效的部分：几何与绘制参数、指针命中判定、起手文件取证思路。

## 结论

Filee 的圆环不是简单点击菜单，而是具有 **拖放模式、点击模式、正反面、分组子环、转换进度和结果反馈** 的完整组件。macOS 默认唤起手势是 **Option + 左键拖动**，不是仅按住 Option 就显示；也支持通过快捷键读取文件管理器选中项后以点击模式打开。

目标是在 ClipBase 中做一个独立 `NSView` 子模块。**可移植的是几何、绘制、命中、状态机和动画；全局手势、文件管理器选择、拖放窗口、预设持久化与转换队列必须由 ClipBase 宿主负责。**

## 证据与标记

证据来源：

1. [Filee 2.1.0 配置页截图](screenshots/filee/config-page.png)（已随文档入库，原会话临时图已不再引用）。
2. [Filee 文件拖放运行态截图](screenshots/filee/drag-runtime.png)。
3. 用户对 Option 唤起、保留源文件、成功动画和按文件类型配置的运行观察。
4. `/Users/dayiteng/Filee` 当前源码树。Filee 使用 C# + Avalonia，自绘 `DonutMenu`，并非 AppKit 原生控件。

标记约定：

- **【源码事实】**：当前源码直接确认，但未在本次会话启动程序复测。
- **【截图事实】**：截图直接可见。
- **【用户运行观察】**：用户实际操作后补充的行为。
- **【ClipBase 设计】**：移植和融合决策，不冒充 Filee 原实现。
- **【待验证】**：依赖用户配置、运行环境或当前机器转换引擎，静态源码不能唯一确定。

---

## 1. 环形工具栏完整交互需求文档

### 1.1 两种打开模式

#### A. 拖放模式

**【源码事实】默认手势：macOS 为 Option + 左键拖动，Windows 为 Ctrl + 左键拖动。** 默认手势定义见 [TriggerGesture.cs:77](file:///Users/dayiteng/Filee/src/Filee.Core/Settings/TriggerGesture.cs#L77)。

调用链：

```text
SharpHook 全局输入
  → TriggerService
  → GestureDetector.DragStarted
  → App.triggers.DragStarted
  → RadialController.ShowForDrag(x, y)
  → 在鼠标位置放置不激活、暂时不可见的窗口
  → 文件 DragEnter
  → RadialViewModel.Load(files, clickMode: false)
  → DonutMenu.PlayOpenAnimation()
```

源码位置：

- 全局事件到手势事件：[TriggerService.cs:40](file:///Users/dayiteng/Filee/src/Filee.App/Services/Triggers/TriggerService.cs#L40)。
- 手势状态机：[GestureDetector.cs:57](file:///Users/dayiteng/Filee/src/Filee.App/Services/Triggers/GestureDetector.cs#L57)。
- 手势接入圆环：[App.axaml.cs:88](file:///Users/dayiteng/Filee/src/Filee.App/App.axaml.cs#L88)。
- 拖放窗口建立：[RadialController.cs:79](file:///Users/dayiteng/Filee/src/Filee.App/Services/RadialController.cs#L79)。

行为规则：

1. 鼠标按下时修饰键和按钮必须与配置精确匹配。
2. 默认拖动阈值取系统值，回退为 4 个物理像素；判断为 `abs(dx) > threshold || abs(dy) > threshold`，见 [GestureDetector.cs:123](file:///Users/dayiteng/Filee/src/Filee.App/Services/Triggers/GestureDetector.cs#L123)。
3. 支持已经开始拖动后再按修饰键，但必须满足配置的按钮、修饰键和作用域，见 [GestureDetector.cs:129](file:///Users/dayiteng/Filee/src/Filee.App/Services/Triggers/GestureDetector.cs#L129)。
4. 手势刚开始时不知道拖的是文件、文本还是框选，因此先建立不可见接收窗口；没有文件进入就不显示圆环。
5. 文件第一次进入后才加载正面、计算可用性并播放展开动画。
6. 10 秒仍没有文件进入会关闭空拖放窗口，见 [RadialController.cs:69](file:///Users/dayiteng/Filee/src/Filee.App/Services/RadialController.cs#L69)。
7. 圆环在鼠标位置居中，但被限制在当前屏幕工作区内；有分组时为子环预留空间，见 [RadialController.cs:497](file:///Users/dayiteng/Filee/src/Filee.App/Services/RadialController.cs#L497)。

#### B. 点击模式

**【源码事实】**快捷键、上下文菜单或拖放区可先取得文件，再以点击模式打开。默认快捷键为 Ctrl + Option + Space；它读取文件管理器当前选中项。

```text
SelectionGesture
  → IPlatformServices.GetFileManagerSelection()
  → RadialController.ShowForFiles(...)
  → RadialViewModel.Load(files, clickMode: true)
  → 激活窗口并播放展开动画
```

点击模式下：

- 单击可用普通扇区直接转换。
- 右键扇区可编辑 preset。
- Esc、点击中心或窗口失焦关闭。
- 窗口失焦存在针对 macOS 跨进程调用的短暂宽限逻辑，见 [RadialController.cs:255](file:///Users/dayiteng/Filee/src/Filee.App/Services/RadialController.cs#L255)。

### 1.2 状态机

```text
Idle
├─ DragGesture → AwaitingFiles（窗口不可见）
└─ SelectionGesture → Open(front, clickMode)

AwaitingFiles
├─ FilesEntered → Open(front, dragMode)
├─ 10s 无文件 → Idle
└─ DragEnded → Idle

Open(face, mode)
├─ FlipKey / VerticalWheel，且 CanFlip → Flipping
├─ PointerMoved → 更新 hover、中心提示和分组状态
├─ Click 普通可用扇区（点击模式） → Progress
├─ Drop 普通可用扇区/子扇区 → Progress
├─ Click 分组 / hover 满足条件 → GroupExpanded
├─ Drop 中心、点击中心、Escape → Closing
├─ DragEnded 在圆环外 → Closing
└─ ClickOutside / Deactivated（点击模式） → Closing

Flipping
├─ 开始时不等待动画，立即把 Items 换成另一面
├─ 旧面只保留用于绘制；命中和 drop 始终使用新面
├─ 再次 Flip → 切换 queued 标记，当前翻面结束后再翻一次
└─ 完成 → Open(otherFace)

Progress
├─ SetProgress → 平滑更新圆弧
├─ 5s 后仍运行 → HandOffToToast
├─ Completed → SuccessBadge → Hold → Fade → Idle
├─ Failed/部分失败 → FailureBadge + Toast → Hold → Fade → Idle
├─ Cancelled → 短暂反馈 → Idle
└─ Escape/关闭请求 → HandOffToToast
```

状态实现集中在 [RadialController.cs:36](file:///Users/dayiteng/Filee/src/Filee.App/Services/RadialController.cs#L36)、[RadialViewModel.cs:16](file:///Users/dayiteng/Filee/src/Filee.App/ViewModels/RadialViewModel.cs#L16) 和 [DonutMenu.Progress.cs:27](file:///Users/dayiteng/Filee/src/Filee.App/Controls/DonutMenu.Progress.cs#L27)。

### 1.3 所有核心操作的预期行为

#### Option 与文件拖入

- **A【必须对齐】**：默认是 Option + 左键拖动，不是仅按 Option。
- **A【必须对齐】**：文件未进入前不显示圆环。
- **A【必须对齐】**：拖入后先显示正面；背面可用性延迟到正面显示后计算。
- **A【必须对齐】**：中心显示文件数量和格式；同一格式显示“数量 + 格式名”，混合格式显示混合数量。
- **A【必须对齐】**：Shapefile 的 `.shp/.shx/.dbf/...` 伴随文件会被归并，不分别计数和转换。

#### 拖过扇区与中心

**【源码事实】**[RadialWindow.axaml.cs:75](file:///Users/dayiteng/Filee/src/Filee.App/Views/RadialWindow.axaml.cs#L75)：

1. 每次 DragOver 重新进行命中测试。
2. 外部拖放高亮与鼠标 hover 分离，外部高亮优先。
3. 只有可用的普通扇区或可用子扇区返回 `DragDropEffects.Copy`。
4. 中心、禁用扇区、分组父项和圆环外返回 `None`。
5. 中心热区显示危险色浅填充，中心副标题变为“拖到这里取消”。
6. 在中心或非转换区域 drop，不启动转换，释放浏览器拖放资源并关闭。
7. 在圆环外松开且未发生有效 drop，等待 250ms 容忍 drop 与 mouse-up 的事件竞态后关闭。

**源文件保护不仅是视觉上的 Copy：**源码明确注释不能报告 Move，否则文件管理器可能在 drop 后删除源文件，见 [RadialWindow.axaml.cs:84](file:///Users/dayiteng/Filee/src/Filee.App/Views/RadialWindow.axaml.cs#L84)。

#### 鼠标 hover

**A【必须对齐】**[DonutMenu.cs:429](file:///Users/dayiteng/Filee/src/Filee.App/Controls/DonutMenu.cs#L429)：

- hover 使用径向距离 + 角度命中，外半径额外允许 `PopOut = 7px`。
- 当前扇区向外移动最多 7px，并额外放大 2.5%。
- 高亮使用强调色，最高不透明度系数为 0.82；文字切换到强调色上的前景色。
- hover 进入和退出采用弹簧动画；退出使用临界阻尼，避免反复回弹。
- Disabled 扇区使用 0.55 倍扇区不透明度，不显示强调色填充，tooltip 显示禁用原因。
- hover 会更新中心提示；拖放模式显示“拖放到……”，点击模式显示“点击以……”。
- 离开控件时清除 hover 并折叠分组，但穿越透明接缝有专门防闪烁处理。

#### 点击与键盘

**A【必须对齐】**[DonutMenu.cs:485](file:///Users/dayiteng/Filee/src/Filee.App/Controls/DonutMenu.cs#L485)：

- 左键按下只记录 press，不立即转换。
- 左键松开时按**松开位置的命中结果**执行；源码没有要求松开位置必须与按下位置相同。
- 普通可用项发出 `SegmentInvoked`，随后进入转换。
- 分组父项不转换；点击时立即展开子环。
- Disabled 项和空子项不执行。
- 点击中心发出 `CenterInvoked`，控制器关闭圆环。
- 点击已展开分组的外部区域先折叠分组。

点击模式还支持：

- 方向键顺时针/逆时针移动高亮。
- Enter 或 Space 转换普通项，或展开分组。
- 子环内 Enter/Space 转换子项。
- Escape/Backspace 先折叠子环；Escape 再次才关闭圆环。

见 [DonutMenu.Groups.cs:203](file:///Users/dayiteng/Filee/src/Filee.App/Controls/DonutMenu.Groups.cs#L203)。

#### 分组子环

**【源码事实】**[DonutMenu.Groups.cs:1](file:///Users/dayiteng/Filee/src/Filee.App/Controls/DonutMenu.Groups.cs#L1)：

- 一个分组最多 6 个非分组 preset。
- 在分组扇区停留 160ms 自动展开。
- 在分组扇区向外移动达到 10px 立即展开。
- 点击分组不等待 hover，立即展开。
- 子环厚度 46px，与主环弹出边缘间距 6px，并额外预留 10px 标签空间。
- 穿过主环与子环之间的缝隙时不折叠，避免 hover 和 drop 闪烁。

#### Tab 与滚轮翻面

**A【必须对齐】**[DonutMenu.Flip.cs:20](file:///Users/dayiteng/Filee/src/Filee.App/Controls/DonutMenu.Flip.cs#L20)：

1. 默认翻转键为 Tab，可在设置中更换；修饰键不妨碍翻转键生效。
2. 只有另一面至少有一个当前输入下可用的扇区时，`CanFlip` 才为 true。
3. 背面为空或全部 Disabled 时，Tab 不翻面，也不会继续移动焦点。
4. 编辑模式和进度模式不允许通过 `Flip()` 翻面。
5. 点击模式由焦点窗口接收按键和滚轮；拖放模式由全局 hook 接收，避免 OS drag 期间窗口无焦点。
6. 拖放模式的全局按键重复不会重复触发；一次按下只发一次翻转事件。
7. 滚轮只检查垂直方向非零，**不区分向上或向下，也不按方向选择正反面**；每次有效滚动都是“翻到另一面”。
8. 300ms 内的连续滚轮事件视为同一次滚动，只翻一次；快速惯性滚动不会来回翻面。
9. 翻面途中再次触发会切换 queued 标记：当前翻面结束后再翻一次；再触发一次会取消这个排队。
10. 动画开始时立即交换数据，命中测试始终针对新面。

#### 翻面动画

**A【必须对齐】**：

- 单扇区折叠时间 `SliceTurn = 0.12s`。
- 波动扩散范围 `Stagger = 0.08s`。
- 总时长 `FlipDuration = 0.20s`。
- 每个扇区绕自己的径向中线折叠，而不是整个圆环统一绕 Y 轴旋转。
- 从鼠标/拖动位置对应扇区开始，向两侧按角距离扩散；没有有效指针位置时从顶部开始。
- 中心圆保持原位。
- 旧面在前半段折出，新面在后半段折入，宽度为 `abs(cos(πu))`，最小绘制系数 0.01。
- 减少动态效果时立即换面，不播放动画。

#### 打开、关闭与重排动画

**【源码事实】**[DonutMenu.cs:92](file:///Users/dayiteng/Filee/src/Filee.App/Controls/DonutMenu.cs#L92)：

- 打开单扇区时长 0.42s，依次延迟 0.028s。
- 打开使用 `EaseOutBack`，从中心缩放并带轻微旋转。
- 关闭单扇区时长 0.16s，依次延迟 0.01s，使用 `EaseInCubic`。
- 编辑器插入或移动 preset 时，其他扇区的中心角和角宽通过弹簧移动，为新项让位。
- 动画帧停滞有重试和最终跳到终态的保护；减少动态效果时直接到终态。

### 1.4 转换进度与成功动画

**A【必须对齐】**[DonutMenu.Progress.cs:1](file:///Users/dayiteng/Filee/src/Filee.App/Controls/DonutMenu.Progress.cs#L1)：

1. drop/click 后圆环原地切换为进度模式，不另开独立成功窗口。
2. 扇区先向中心收缩：每项 0.28s，逐项间隔 0.02s，并伴随旋转和淡出。
3. 随后出现进度环：半径 36px、线宽 6px，中心显示百分比。
4. 进度圆弧以指数平滑追赶真实进度。
5. 圆环至少显示 0.6s；极快任务也不会来不及显示成功勾。
6. 成功后圆弧填满，中心弹出绿色圆形 ✓；badge 使用过冲弹簧。
7. 成功保持 0.9s，再以 0.25s 淡出并隐藏窗口。
8. 失败显示 `!`，保持 1.4s，同时 Toast 展示错误；部分失败也走失败反馈。
9. 取消保持 0.2s。
10. 任务 5s 后仍运行，圆环用 0.22s 收缩并交给 Toast，不阻塞后续手势。
11. 进度中 hover 可显示取消按钮；点击已完成的 ✓ 可打开输出目录。
12. 减少动态效果时直接切换视觉状态，但结果保持时间仍保留。

### 1.5 A/B 对齐边界

#### A. 必须原样对齐 Filee 的交互规则

- Option + 拖动和点击模式的状态差异。
- 文件进入后才显示、正面优先、背面延迟构建。
- 正反面独立、每面最多 12、只有另一面可用才允许翻转。
- Tab/配置键和垂直滚轮的翻转语义、300ms 去抖、翻面重入规则。
- hover、禁用态、点击松开时触发、中心取消、分组展开规则。
- drop 必须使用 Copy 语义并保护源文件。
- 原地进度环、成功/失败反馈、5 秒 Toast 接管规则。
- 新面立即参与命中，不能等翻面动画结束才更新数据。

#### B. 可以适配 ClipBase 的部分

- 配色、材质、阴影、字体和圆角。
- 外半径、孔径比例、间距和控件尺寸。
- 弹簧刚度、阻尼的等效 AppKit 实现方式。
- 使用 `NSView`、`CAShapeLayer` 或 `NSBezierPath` 的具体渲染技术。
- Toast、错误页面和输出目录揭示的宿主 UI。
- VoiceOver、减少动态效果和 ClipBase 自身快捷键设置。

---

## 2. 布局数学计算逻辑

以下均为 [DonutGeometry.cs:45](file:///Users/dayiteng/Filee/src/Filee.App/Controls/DonutGeometry.cs#L45) 和 `DonutMenu` 的真实算法。

### 2.1 角度和扇区分配

Filee 使用**从 12 点开始、顺时针增加的弧度**，不是标准数学坐标角：

```text
Span(N) = N <= 0 ? 2π : 2π / N
MidAngle(i, N) = i × Span(N)
```

所以第一个扇区以正上方为中心，后续顺时针排列。

点坐标：

```text
PointAt(center, radius, angle):
x = center.x + radius × sin(angle)
y = center.y - radius × cos(angle)
```

点的角度：

```text
angle = atan2(point.x - center.x, center.y - point.y)
if angle < 0: angle += 2π
```

扇区索引：

```text
span = Span(N)
index = floor((angle + span/2) / span) mod N
```

`+span/2` 是因为 `MidAngle` 表示扇区中心，而不是起始边。

### 2.2 半径、数量和尺寸

- 默认外半径 `R = 150`，加载设置时限制在 100...240。
- 默认孔径比例 `HoleRatio = 0.42`。
- 内半径 `r = R × clamp(HoleRatio, 0.2, 0.75)`。
- 每面最多 12 项。
- `N = 1` 时绘制完整圆环，不画无意义的单扇区分割。
- `N = 0` 时绘制空圆环，编辑器可接收第一个 preset。
- 主环外侧为 hover 弹出预留 7px，为阴影预留 14px。
- 子环额外预留 `6 + 46 + 10 = 62px`。

窗口边长：

```text
SideFor(R, hasSubRing) =
2 × (R + 7 + 14 + (hasSubRing ? 62 : 0))
```

### 2.3 恒定像素间隙

Filee 不是直接给所有半径使用相同角间隙，而是分别校正内外半径，使间隙约为固定 3px：

```text
GapPixels = 3
outerHalf = max(0.001, span/2 - GapPixels/(2R))
innerHalf = max(0.001, span/2 - GapPixels/(2r))
```

然后使用：

```text
outerStart = PointAt(C, R, mid - outerHalf)
outerEnd   = PointAt(C, R, mid + outerHalf)
innerStart = PointAt(C, r, mid - innerHalf)
innerEnd   = PointAt(C, r, mid + innerHalf)
```

外弧顺时针、内弧逆时针闭合。

圆角首先受 `(R-r)/2` 限制；过薄或过短时，通过最多 14 次二分缩小找到可放置的圆角。圆角只切削视觉轮廓，不改变扇区间隙。

### 2.4 命中测试

真实实现不是 Path contains：

```text
distance < r                 → Center
distance > R + PopOut        → Outside
N == 0                       → EmptyRing
otherwise                    → Segment(IndexAt(AngleOf(...)))
```

见 [DonutGeometry.cs:115](file:///Users/dayiteng/Filee/src/Filee.App/Controls/DonutGeometry.cs#L115)。

**关键对齐点：被圆角切掉的角仍属于原扇区，扇区间隙也没有单独的角度命中分支。** ClipBase 不应擅自改成“精确圆角 Path 命中”，否则会改变 Filee 的边缘交互。

### 2.5 标签定位与防重叠

主标签锚点：

```text
labelRadius = (R + r) / 2
anchor = PointAt(center, labelRadius, mid)
maxWidth = clamp(span × labelRadius - 10, 30, 120)
maxHeight = R - r - 16
```

文本规则见 [DonutMenu.cs:995](file:///Users/dayiteng/Filee/src/Filee.App/Controls/DonutMenu.cs#L995)：

- 标签始终水平、居中，不随扇区旋转。
- 主标题最多两行。
- 默认字号 13，最小 10。
- 对不含汉字/假名的空格分词，测量最宽单词；超宽时按比例缩字，避免英文单词从中间断开。
- 中文、日文和 CJK 符号允许字符间断行。
- 韩文单词插入 word joiner，避免不期望的韩语音节断行。
- 主标题与 caption 相同（忽略大小写）时不显示 caption。
- 只有 `labelHeight + 14 < maxHeight` 才显示 caption。
- caption 字号 10.5；高度足够时最多两行，否则一行。
- 最终溢出使用字符省略号。

### 2.6 数量变化、主题和缩放

- 文件变化会重新选择 profile，正面立即重建，背面按需重建。
- 当前 `Items` 改变时重置 hover、drop preview、子环和旧面状态。
- 编辑器预览通过改变每个扇区的 `mid/span` 目标值实现平滑重排。
- 外半径来自设置；窗口以内容尺寸重算并限制在屏幕工作区。
- `Motion.Enabled == false` 时所有弹簧和翻转直接进入终态，但业务状态转换保持不变。
- 主题画刷与几何数据分离，可作为 ClipBase 样式注入点。

---

## 3. 数据结构、配置匹配与持久化

### 3.1 Filee 的真实三层模型

#### 持久化配置：ToolbarProfile

[ToolbarProfile.cs:10](file:///Users/dayiteng/Filee/src/Filee.Core/Profiles/ToolbarProfile.cs#L10)：

```csharp
ToolbarProfile {
    Id
    Name
    NameKey?
    Extensions: List<string>
    IsFallback: Bool
    PresetIds: List<string>       // 正面，顺时针
    BackPresetIds: List<string>?  // 背面，顺时针
    const MaxSlices = 12
}
```

关键不变量：

- 正反面分别保存 preset ID，不复制完整 preset。
- `BackPresetIds` 为空或没有可用项时不翻面。
- `null` 主要用于旧配置迁移，不等于新设计中的第三面。
- 扩展名不带点，匹配忽略大小写。
- 普通 profile 与 mixed fallback 使用同一结构，由 `IsFallback` 区分。

#### 持久化动作：Preset

[Preset.cs:70](file:///Users/dayiteng/Filee/src/Filee.Core/Presets/Preset.cs#L70)：

- `Kind = Convert | Group | Tool`。
- 普通转换包含目标格式、图片/PDF/文档/媒体/压缩等选项和输出规则。
- 分组使用 `SubPresetIds`，最多 6 个，成员不能继续是 group。
- `SameAsSource = "*"` 是特殊目标标记，在每个文件执行时独立解析，不是真实扩展名。
- 入队时克隆 preset，之后编辑预设不会改变已运行任务。

#### 运行时显示：DonutItem

[DonutMenu.cs:23](file:///Users/dayiteng/Filee/src/Filee.App/Controls/DonutMenu.cs#L23)：

```csharp
DonutItem {
    Id
    Label
    Caption?
    IsEnabled
    DisabledReason?
    Children: IReadOnlyList<DonutItem>?
}
```

`RadialViewModel.BuildFace` 负责：

```text
PresetIds
  → FindPreset
  → 忽略已删除 ID
  → PresetAvailability.Check
  → 生成 DonutItem
  → group 解析 Children
```

正面立即构建；背面先保存 ID，首次需要时构建，见 [RadialViewModel.cs:146](file:///Users/dayiteng/Filee/src/Filee.App/ViewModels/RadialViewModel.cs#L146)。

### 3.2 文件类型与混合选择分支

**【源码事实】**[ToolbarProfile.cs:63](file:///Users/dayiteng/Filee/src/Filee.Core/Profiles/ToolbarProfile.cs#L63)：

1. 对所有输入调用 `FormatRegistry.ExtensionOf`。
2. 扩展名忽略大小写去重。
3. 选择第一个满足“**所有输入扩展名都匹配**”的普通 profile。
4. 找不到时，选择第一个扩展名非空且能覆盖全部输入的 `IsFallback` profile。
5. 再找不到时，选择第一个扩展名为空的 fallback。
6. 若无通用 fallback，则依次退到第一个 fallback、第一个 profile。

因此，“用于混合选择”不是为每个文件选不同圆环，而是为跨类型输入选择 fallback 圆环。

可用性与执行还需分开：

- 普通转换 preset 的可用性通常使用 `formats.Any(...)`：至少一种输入可转即可启用。
- 整个选择仍使用同一个 preset 创建一个 job。
- 每个文件独立检测格式和规划转换路由。
- 不支持的单个文件失败，其他文件继续。
- 部分成功最终为 `CompletedWithErrors`，圆环按失败反馈处理并交给 Toast 解释。
- PDF 合并、多页 TIFF、媒体拼接和整包压缩有各自的汇聚分支，不能套用普通逐文件规则。

### 3.3 搜索、分组和持久化

- 搜索先 `Trim`，使用当前区域文化的忽略大小写 `Contains`。
- preset 搜索名称、目标说明、目标格式。
- profile 搜索名称和扩展名。
- 可用预设面板搜索 label/caption，再叠加分类 tab 和可用性过滤。
- 列表通过 Remove/Move/Insert 增量更新，而不是清空重建，见 [FilteredList.cs:8](file:///Users/dayiteng/Filee/src/Filee.App/ViewModels/FilteredList.cs#L8)。

[UserDataStore.cs:39](file:///Users/dayiteng/Filee/src/Filee.Core/Settings/UserDataStore.cs#L39) 使用四个独立 JSON：

```text
settings.json
presets.json
profiles.json
history.json
```

保存 library 时：

1. 清理正反面 dangling ID、去重并限制每面 12 项。
2. 清理分组成员、去重并限制 6 项。
3. 保证至少一个 fallback。
4. 先写 `.tmp`，再原子替换目标文件。
5. 发出 `LibraryChanged`。

这些是宿主持久化规则，不应放入 `NSView`。

---

## 4. 剩余探查与验证清单

源码已回答原文档中的 hover、点击、滚轮、动画、混合选择、几何和持久化问题，**无需再用 Hopper/Ghidra 猜测这些逻辑**。剩余工作是运行验证和移植验证。

### 4.1 Filee 运行验证

1. 读取当前用户实际 `settings.json`，确认是否仍使用默认 Option + 左键拖动；自定义配置不能由默认值替代。
2. 验证触控板惯性滚动在 300ms 去抖边界附近的实际体验。
3. 验证快速连续 Tab 的 queued 切换和指针下扇区波前效果。
4. 验证多文件、未知格式和部分失败时 Toast 与圆环反馈。
5. 验证屏幕边缘、多显示器、不同 DPI 下窗口位置。
6. 验证系统“减少动态效果”和深浅色实时切换。
7. 验证当前机器实际可用转换引擎；最终 route 取决于引擎安装状态、优先级和可选包，静态源码不能唯一确定。

### 4.2 ClipBase 移植验证

1. macOS 全局手势需要辅助功能权限；失败时仍应有点击模式入口。
2. `NSDraggingDestination` 的进入、更新、离开和 drop 顺序要与 Filee 状态机对齐。
3. AppKit 事件不能重复处理：拖放模式全局 Tab 与窗口本地 Tab 只能保留一个来源。
4. 用几何单元测试覆盖 N = 0、1、2、10、12，以及跨 12 点角度取模。
5. 验证翻面动画期间 drop 命中的是新面。
6. 验证圆角区域仍按原扇区命中，不擅自改成 Path contains。
7. 验证转换不修改源文件，输出重名时生成新名称。
8. 验证进度超过 5 秒、取消、失败、部分失败和新手势打断时的宿主状态。

可复用 Filee 现有测试作为行为参照：

- `tests/Filee.App.Tests/DonutGeometryTests.cs`
- `tests/Filee.App.Tests/RadialControllerTests.cs`
- `tests/Filee.Core.Tests/ProfileSelectorTests.cs`
- `tests/Filee.Core.Tests/BackFaceTests.cs`
- `tests/Filee.Core.Tests/ReplaceOriginalTests.cs`
- `tests/Filee.Core.Tests/JobQueueTests.cs`

这些测试本次未运行，只作为后续移植测试的参照位置。

---

## 5. ClipBase 融合约束说明

> **时效说明（2026-10-26 收尾）**：本章写于 ClipBase 还是 AppKit 原生实现的阶段，
> 讨论的是「如何在 NSView 里嵌 Filee 的 Avalonia 控件」。当时并行的原生实现已废弃，
> 现在的环形盘是 Tauri webview 里的 React 组件（`src/windows/RingWindow.tsx`
> + `src/lib/ringGeometry.ts`），宿主只提供「Finder 选中项快照 + 执行预设」两件事
> （`src-tauri/src/core/ring/mod.rs`）。下面的 NSView 结论仅作历史参考，**不要照此实现**。

### 5.1 当前 ClipBase 的真实边界（AppKit 时期）

- 原生面板挂载独立 NSView（原 `native/Sources/ClipBase/PanelViewController.swift`，已移除）。
- 原生 `AppService` 当时没有文件转换服务。
- 文件转换引擎始终在 Rust 侧，格式支持见 `src-tauri/src/core/convert/format.rs`。
- 当时没有转换预设库；`ConfigStore` 是应用总配置存储，不是 preset 系统。
- Web 端有主题变量，原生端尚无完整统一主题管理。

因此，**不能直接把 Filee 的 Avalonia 控件塞入 NSView。** 必须先由宿主提供转换协调接口
（当前实现：环形盘通过 `ring_snapshot` / `ring_execute` 与宿主交互）。

### 5.2 独立 NSView 的边界

建议拆成：

```text
RingWheelView : NSView
├── 当前面的 RingItemViewModel 列表
├── 几何、绘制、命中、hover、键盘、滚轮和翻面
├── 分组子环
├── 进度和结果动画
└── 只通过回调向宿主发出意图

RingWheelHosting（宿主）
├── NSPanel/窗口位置、透明拖放接收和屏幕约束
├── Option 手势、文件管理器选中项和权限
├── profile 选择、preset 解析和可用性
├── 文件 drop 数据读取
├── 转换任务、取消、进度、Toast 和输出目录
└── 配置及预设持久化
```

建议视图回调：

```swift
onHoverChanged: (Int?) -> Void
onFlipRequested: () -> Void
onItemActivated: (Int, Int?) -> Void
onCenterActivated: () -> Void
onDrop: ([URL], Int, Int?) -> Void
onProgressCancel: () -> Void
onProgressOpenOutput: () -> Void
```

`onFlipRequested` 是否直接由视图内部换面，可在实现时收敛，但必须保持 Filee 的“数据立即切换、旧面仅用于绘制”语义。

### 5.3 NSView 明确不得承担

- 创建独立 App、AppDelegate、主窗口或转换引擎。
- 安装进程级全局 hook。
- 读取 Finder 选中项或管理辅助功能权限。
- 直接访问 SQLite、JSON、UserDefaults、Filee 配置或 ClipBase `ConfigStore`。
- 直接调用 Rust/Tauri 命令。
- 决定输出路径、覆盖源文件或删除输入。
- 持有批量转换任务和文件生命周期。
- 伪造 ClipBase 尚不支持的 PDF、EPUB、HWPX、DOCX 等转换能力。

### 5.4 源文件保护约束

**A【必须对齐】**：

1. 拖放效果使用 Copy，不使用 Move。
2. 转换器不得修改输入文件。
3. 默认输出到源文件旁的新文件，重名时使用 `name (2).ext` 等避让。
4. 即使输出策略允许覆盖，也不能直接覆盖源路径。
5. ClipBase 初版不应加入 Filee 的 `ReplaceOriginal` 分支；用户当前期望是保留源文件。

Filee 对应保护见：

- [IConverter.cs:145](file:///Users/dayiteng/Filee/src/Filee.Core/Conversion/IConverter.cs#L145)：转换器不得修改输入。
- [OutputPathResolver.cs:46](file:///Users/dayiteng/Filee/src/Filee.Core/Presets/OutputPathResolver.cs#L46)：任何策略都不得直接覆盖源路径。

### 5.5 可复用源码与调用清单

| 能力 | Filee 位置 | ClipBase 用法 |
|---|---|---|
| 扇区角度、坐标、命中、圆角轮廓 | [DonutGeometry.cs:45](file:///Users/dayiteng/Filee/src/Filee.App/Controls/DonutGeometry.cs#L45) | 逐公式移植为纯 Swift geometry，优先做单元测试 |
| 主环绘制、标签、hover、点击 | [DonutMenu.cs:49](file:///Users/dayiteng/Filee/src/Filee.App/Controls/DonutMenu.cs#L49) | 作为 NSView 行为和绘制参照，不直接引用 Avalonia |
| 正反面和滚轮 | [DonutMenu.Flip.cs:20](file:///Users/dayiteng/Filee/src/Filee.App/Controls/DonutMenu.Flip.cs#L20) | 移植状态机、300ms 去抖和翻面时序 |
| 分组子环和键盘 | [DonutMenu.Groups.cs:20](file:///Users/dayiteng/Filee/src/Filee.App/Controls/DonutMenu.Groups.cs#L20) | 第二阶段移植；不与基础转换耦合 |
| 进度和成功动画 | [DonutMenu.Progress.cs:27](file:///Users/dayiteng/Filee/src/Filee.App/Controls/DonutMenu.Progress.cs#L27) | 视图只消费宿主进度和最终状态 |
| Option/拖动手势识别 | [GestureDetector.cs:16](file:///Users/dayiteng/Filee/src/Filee.App/Services/Triggers/GestureDetector.cs#L16) | 移植到宿主机手势服务，不放进 NSView |
| 拖放窗口和转换协调 | [RadialController.cs:36](file:///Users/dayiteng/Filee/src/Filee.App/Services/RadialController.cs#L36) | 作为宿主 coordinator 的调用顺序参照 |
| 正反面运行时构建 | [RadialViewModel.cs:146](file:///Users/dayiteng/Filee/src/Filee.App/ViewModels/RadialViewModel.cs#L146) | 用 ClipBase preset repository + probe 重写 |
| 文件类型和 mixed 选择 | [ToolbarProfile.cs:63](file:///Users/dayiteng/Filee/src/Filee.Core/Profiles/ToolbarProfile.cs#L63) | 可移植纯选择算法，数据由 ClipBase 持有 |
| 输出路径保护 | [OutputPathResolver.cs:24](file:///Users/dayiteng/Filee/src/Filee.Core/Presets/OutputPathResolver.cs#L24) | 保留安全规则；ClipBase 当前已有基础重名避让 |
| Filee 转换队列 | [JobQueue.cs:58](file:///Users/dayiteng/Filee/src/Filee.Core/Conversion/JobQueue.cs#L58) | 不直接引入；只参考任务状态和取消语义 |
| Filee JSON 持久化 | [UserDataStore.cs:48](file:///Users/dayiteng/Filee/src/Filee.Core/Settings/UserDataStore.cs#L48) | 仅作导入/迁移参照，不与 ClipBase 默认共享写文件 |

### 5.6 实施顺序约束

1. 先移植 `DonutGeometry` 和基础 `RingWheelView`，完成 N = 0/1/10/12 测试。
2. 接入 ClipBase 宿主的 profile/preset 解析和现有转换 probe。
3. 完成点击模式转换，再实现 Option + 拖放模式，避免同时调试全局 hook 和视图。
4. 加入正反面、中心取消和源文件保护验收。
5. 接入真实进度与成功/失败动画。
6. 最后加入分组子环和编辑器拖拽重排。

**组件不能独立跑；所有文件、转换、存储和全局事件都必须来自 ClipBase 宿主。**
