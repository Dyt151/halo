# Filee 格式转换专项逆向与 ClipBase 对齐规范

> 范围：只研究“文件和预设如何变成转换结果”，不扩展圆环手势、编辑器或成功动画。
>
> 证据：Filee 与 ClipBase 当前源码的静态阅读。未运行两款应用，未验证实际图像质量、动画保真度、运行时取消时序或 UI 手感。
>
> 目标：找出 ClipBase 当前转换行为的根因，给出可嵌入现有 Rust/Tauri 架构的模块边界、数据结构和验收标准；不是另写一个独立 App。

## 1. 核心结论

ClipBase 的问题不只是“支持的格式少”，而是把转换建模成了：

```text
源扩展名 + 目标格式字符串 + 全局 JPG 质量 → 直接解码/编码 → 立即写最终文件
```

Filee 的模型是：

```text
完整 Preset 快照
  → ConversionJob
  → 每个文件单独检测格式和规划 Route
  → 一个或多个 ConversionStep
  → 中间产物进入 Job 独立工作目录
  → 最终路径统一分配
  → 输出收尾、失败清理、逐文件结果与历史记录
```

因此，仅在现有 `can_convert_to` 矩阵里增加格式，不能解决以下问题：

- 两个同为 JPG 的动作无法拥有不同质量、缩放或命名规则。
- 同格式压缩、缩放、灰度等合法任务被一律拒绝或跳过。
- 不支持的动作或无路由文件在批量执行中被静默忽略。
- 动画、EXIF 方向、ICC、元数据和透明通道可能被静默丢失。
- 失败可能留下不完整的最终文件。
- 并发后，仅凭 `exists()` 不能避免同名输出竞争。
- 圆环取消只取消选择盘，不能取消已经运行的转换。

正确的改造方向是把“动作 ID = 目标格式”升级为“动作 ID = Preset ID”，并让转换核心独立于圆环。

---

## 2. Filee 转换调用链

### 2.1 入口不是转换器，而是 Job

`ConversionService.Start` 做入口约束：

1. 空文件列表不启动任务。
2. `Group` 和 `Tool` 不能作为普通转换直接入队。
3. 解析默认输出位置。
4. 浏览器拖入的虚拟文件先经 Fetcher 落盘，再进入转换。
5. 调用 `JobQueue.Enqueue(files, preset, displayName, fetcher)`。
6. UI 只接收 `JobViewModel`，用于进度、Toast、历史和重试。

圆环最终只应提交“文件快照 + preset ID”，不应自己拼接图片编码、文档转换或输出路径逻辑。

### 2.2 Preset 是完整配方

`Preset` 不只是 `TargetFormat`，还包含：

- `Image`：质量、缩放、旋转/翻转、DPI、元数据、ICC、压缩、灰度、透明裁剪、背景处理、动画速度等。
- `Pdf`：合并、页面尺寸、DPI、页范围、拆页、旋转、压缩、OCR、水印、页码、密码等。
- `Document`：文档相关选项。
- `Media`：质量、最大高度、码率、声道、采样率、编解码器、硬件编码、逐帧、Live Photo、剪辑、拼接和幻灯片等。
- `Archive`、`Trace`、`Cad`：对应领域的选项。
- `Output`：目录、文件名、冲突、重名、压缩结果、替换源文件等。
- `Kind`：Convert、Group、Tool。
- `SubPresetIds`：分组内容。

`SameAsSource` 使用特殊目标值 `"*"`。它表示“目标按每个源文件自己的格式解析”，不是“无需转换”。例如 JPG 压缩、PNG 缩放、视频同格式降码率都属于合法任务。

`ConversionJob` 创建时执行 `preset.Clone()`。因此，用户在任务运行后编辑设置，不会改变已经排队或正在执行的任务。

### 2.3 转换器只声明直接边

`IConverter` 的核心职责：

```text
Id / DisplayName
Edges
MaxParallelism
GetStatus()
ConvertAsync(step, progress, cancellationToken)
```

引擎只回答“我能直接完成哪些边”，不承担整个格式矩阵，也不负责决定所有最终输出规则。

`ConversionStep` 给引擎提供：

- 当前输入路径。
- 当前 `From` / `To`。
- Preset 快照。
- 输出分配器。
- Job 工作目录。
- 特殊模式、通知和字体缺失信息。
- 原始源文件路径。

引擎契约还包括：支持取消、按自身并发上限线程安全、绝不修改输入文件。

### 2.4 RoutePlanner 负责组合引擎

`ConverterCatalog`：

- 注册并枚举引擎。
- 后台检查和缓存引擎可用状态。
- 刷新时使状态缓存失效。
- 单个引擎状态检查抛错只让该引擎不可用，不拖垮整个目录。
- 创建只包含当前可用引擎的 `RoutePlanner`。
- 将用户优先级转换为每步 0…9 的额外成本。

`RoutePlanner`：

- 以格式为节点、转换器边为有向加权边。
- 用 Dijkstra 选择最低成本路径。
- 普通路由最多 3 步。
- 引擎优先级用于影响成本接近的选择，不等于手工写死完整路线。
- 源格式等于目标格式时，只选择显式 self edge；不能默认复制，也不能默认认为非法。
- `FrameSequence`、`MotionPhoto`、`LivePhoto`、`Ocr` 等特殊边必须按专门模式规划，不能意外进入普通链路。
- `DirectOnly`、图像来源限制等约束参与选路。

示例：OCR 不必由某一个引擎独立完成，可以规划成“图片 → PNG → searchable PDF → DOCX/TXT/MD/HWPX”等路线。

### 2.5 预设可用性与实际执行分开

`PresetAvailability` 用于圆环扇区和设置页状态：

- 普通 preset 对一批文件采用“至少一个文件可执行即可用”的判断。
- `SameAsSource` 按每个源格式分别解析。
- 同格式仍通过 Planner 判断是否存在 self edge。
- 区分“当前无路由”和“安装可选引擎包后可用”。
- 旋转/翻转任务必须找到真正支持变换的路由；仅能复制或重编码但不能变换，不算可用。
- 无透明通道、不能旋转等原因使用不同错误语义。

批次级“可用”不意味着逐个文件静默丢弃不支持的输入。真正执行时仍逐文件规划并记录结果。

---

## 3. 单文件执行状态机

### 3.1 主流程

```text
Pending
  → Running
  → 检测格式
  → 拒绝空文件
  → 解析 SameAsSource
  → 计算 frames / motion / live / ocr 模式
  → Plan Route
  → 校验变换、媒体等约束
  → 逐步执行 ConversionStep
  → PDF finish / ZIP / 单输出位置调整等收尾
  → OnlyIfSmaller / ReplaceOriginal / 日期处理
  → Done 或 Skipped

任意执行阶段
  → Cancelled
  → 清理该文件拥有且未完成的输出

任意执行阶段
  → Failed
  → 保存结构化错误
  → 清理该文件拥有且未完成的输出
```

### 3.2 中间产物和最终输出

- 每个 Job 有独立临时工作目录。
- 路由非最终步骤使用临时输出分配器。
- 只有最后一步使用最终输出分配器。
- 多步路由的中间文件不直接暴露为最终转换结果。
- 收尾和失败路径有输出清理逻辑；不能据此额外宣称 Filee 对所有最终写入都提供文件系统级原子事务。

ClipBase 应进一步明确：引擎先写同目录临时文件或受管工作文件，完整成功并关闭句柄后再提交到最终路径；失败、取消和进程异常恢复都要能清理半成品。

### 3.3 进度

Filee 将单步进度映射到整个路由区间：

```text
floor + (ceiling - floor) × (stepIndex + stepProgress) / stepCount
```

同时只接受前进后的进度，避免异步回调乱序造成倒退。Job 总进度是各文件进度的平均值。

这意味着“完成文件数 / 文件总数”只能算粗粒度批量进度，不能冒充单文件转换进度。

### 3.4 并发与取消

- `JobQueue` 串行处理 Job。
- 单个 Job 内文件可并行，文件并行度受处理器数量限制。
- 每个引擎另有 `MaxParallelism` 信号量。
- 每个 Job 有独立取消令牌。
- 浏览器来源的 Fetch 同样属于 Job 生命周期。

ClipBase 不必照抄“Job 永远串行”，但必须区分三层：Job 调度、文件并行、引擎并行。只给整个函数套一个线程无法提供可取消、可重试和可观测的任务语义。

---

## 4. 输出、重名和源文件保护

### 4.1 OutputRule

Filee 支持：

- 输出位置：默认、源目录、子目录、自定义目录。
- 冲突策略：Rename、Overwrite、Skip。
- 文件名 token：`{name}`、`{ext}`、`{preset}`、`{date}`、`{time}`、`{index}`。
- 多步重命名、保留目录结构、保留日期。
- `ZipResult`、`OnlyIfSmaller`、`ReplaceOriginal`。
- 默认冲突策略为 Rename，典型结果为 `name (2).ext`。

文件名会清理非法字符、控制字符和尾部点；空名称有 fallback。正则替换带超时，非法或超时不能导致原文件意外丢失。

### 4.2 源文件永不因冲突策略被覆盖

关键规则：

```text
候选路径 == 源路径
  → 即使 Conflict == Overwrite，也不能直接覆盖
  → 进入编号/改名分支
```

`ReplaceOriginal` 是另一个显式操作，不是 Overwrite 的隐式副作用。它先处理原文件，失败时尽量保留源，不能让普通转换自动拥有删除源文件的能力。

### 4.3 并行输出必须预留

仅检查“磁盘上当前是否存在”存在竞态：两个并行文件可以同时得到同一个空闲路径。

`IOutputAllocator` 将路径分配和预留从引擎中分离。取消、失败或 Skip 时还要释放/丢弃相应预留，且只能清理由当前文件拥有、未完成、未与其他结果共享的输出。

ClipBase 即使暂时保持批量串行，也应先建立这个边界；否则后续加并发会重新引入数据竞争。

---

## 5. 不能简化成格式矩阵的特殊规则

### 5.1 图像

- 旋转/翻转必须在路由中第一个支持变换的步骤执行，且只执行一次。
- 路由只能复制或普通重编码时，不能输出“未旋转但显示成功”的文件。
- 动画速度只在最终输出步骤改变一次，避免多步路由重复变速。
- 透明图转无 alpha 格式需要明确背景合成策略。
- 多帧、EXIF 方向、ICC、元数据不是扩展名转换天然会保留的属性。
- 多页 TIFF、视频逐帧、Live/Motion Photo 是独立模式，不是普通图片边。

### 5.2 PDF 与文档

- PDF 完成器可在不同引擎生成 PDF 后统一添加水印、页码和密码。
- 多 PDF 合并是批次流程，不能按每个输入各产出一个 PDF 的普通逻辑处理。
- OCR 是特殊路由能力，不能由“图片可转文档”推导出来。
- 页范围、拆页、旋转和合并会改变输出数量与输出分配方式。

### 5.3 媒体与归档

- 媒体拼接、图片生成视频是批次流程。
- 归档合并把多个输入变成一个结果。
- 多输出、单输出和逐文件输出必须由 Job 的执行计划表达，不能全塞进 `convert_file(...) -> one path`。

---

## 6. ClipBase 当前实现对照

| 领域 | Filee 源码事实 | ClipBase 当前事实 | 后果 |
|---|---|---|---|
| 动作身份 | Preset ID，包含目标、参数和输出规则 | `RingAction.id` 就是 jpg/png 等格式字符串 | 无法表达同目标不同参数 |
| 配置 | 每个 preset 自有选项 | `jpeg_quality`、`dest_dir` 是整个 RingConfig 的全局值 | 修改一个动作会影响所有同目标动作 |
| 任务快照 | Job 深拷贝 preset | Tauri API 只传目标、质量和目录 | 无稳定任务配方和历史重试语义 |
| 格式能力 | 引擎边 + 图路由 | 静态象限矩阵 | 能力声明与真实引擎行为容易脱节 |
| 多步转换 | Dijkstra，普通最多 3 步 | 无 Route 或 ConversionStep | 无法组合引擎，也无法控制中间产物 |
| 同格式 | 显式 self edge 决定 | `can_convert_to` 返回 false，执行入口也直接报错/跳过 | 同格式压缩、缩放等不可表达 |
| 图片执行 | 由具体引擎和 preset 处理选项 | `image::open` 成 `DynamicImage` 后直接编码 | 普通路径无法保留完整动画，EXIF/ICC/元数据无显式语义 |
| 图片转文档 | 由具体边定义真实行为 | MD/HTML 生成引用源图片的文档 | 不是图片嵌入或页面渲染，名称容易误导 |
| 批量文件 | 每个输入都有结果 | 无路由/不能识别文件进入 `_ => {}` | 输入被静默忽略，failed 也不包含它们 |
| 批量进度 | 文件进度聚合，步骤内进度单调 | 已完成文件数 / 可转换文件数 | 忽略文件后总数与真实输入语义不一致 |
| 输出写入 | 分配器、工作目录、失败清理 | `File::create` / `fs::write` 直接写最终路径 | 失败可能留下半成品 |
| 重名 | 统一分配并支持并行预留 | 只按当前磁盘 `exists` 追加编号 | 加并发后可能撞名 |
| 源保护 | Overwrite 也不能覆盖源；替换源是独立选项 | 当前同格式被拒绝，尚不能覆盖完整 preset 场景 | 引入同格式后必须先补保护 |
| 并发 | Job 串行、文件并行、引擎限流 | 批量逐项串行，单次在线程中运行 | 没有统一 Job/引擎调度边界 |
| 取消 | Job 级 CancellationToken | `cancel()` 只关闭选择盘 | 运行中的转换不可取消 |
| 结果 | FileResult + JobState + 错误 + 输出 | converted/skipped/failed/errors/output 聚合 | 缺少逐文件状态、取消和明确跳过原因 |
| 可用性 | 按 preset 和实际路由判断 | 按静态矩阵判断，并预先跳过同格式 | 圆环显示状态与真实能力可能不一致 |

另外，空状态仍有“TXT 只能作为输出”的提示，但当前矩阵已允许 TXT → MD/HTML。该文案与实现不一致，应在转换行为修正时同步核实。

---

## 7. 改造优先级

### P0：先修正确性和数据安全

1. **引入 Preset ID**：圆环动作保存 preset ID，不再保存裸目标格式；每个 preset 自有目标、参数和输出规则。
2. **Job 保存 preset 快照**：启动后设置变化不影响已排队任务，重试使用当时配方。
3. **每个输入必须有结果**：converted、skipped、failed、cancelled 必须可解释；禁止 `_ => {}` 静默丢弃。
4. **安全写入**：成功前不暴露完整最终文件；失败/取消清理半成品。
5. **统一输出分配**：源路径永不因 Overwrite 被覆盖；路径分配要支持预留，Skip 不能留下空文件。
6. **同格式按能力判断**：只有 preset 明确需要重编码/处理且引擎支持时才可执行；无 self edge 时给出明确原因。
7. **明确图像语义**：透明背景合成有确定策略；动画、EXIF、ICC、元数据不支持时必须声明限制或拒绝，不可静默当作完整保真成功。
8. **校验结果真实性**：图片 → MD/HTML 应明确命名为“生成引用源图的文档”，或实现真正嵌入/渲染；不能用模糊能力声明误导用户。

### P1：建立可扩展执行模型

1. `ConversionJob`、`ConversionFileResult`、`JobState`。
2. Job 级取消令牌和运行中任务注册表。
3. 单文件/单步单调进度，Job 聚合进度。
4. `ConverterEngine`、`ConversionEdge`、`ConversionStep`。
5. 引擎可用状态和每引擎并发上限。
6. 按 preset 与真实能力判断圆环可用性。
7. RoutePlanner、Job 工作目录和中间产物生命周期。
8. 历史与重试绑定 Job 快照，而不是当前全局设置。

### P2：再扩格式和高级能力

- PDF、Office、音视频、OCR、电子书等更多引擎。
- 多步路由、多页、多输出、合并、拼接和归档。
- 可选引擎包的下载、安装原因和状态提示。

不要把“增加扩展名数量”放在 P0 数据安全之前。

---

## 8. 推荐的 ClipBase 模块边界

以下是接口规范，不是要求一次性实现全部 Filee 格式。

### 8.1 ConversionPreset

```text
ConversionPreset
- id: stable string
- name: localized/display string
- target: TargetSpec            // Explicit(format) | SameAsSource
- image: ImageOptions
- document: DocumentOptions     // 只放当前真实支持的字段
- output: OutputRule
```

约束：

- 不支持的高级字段不要先做空壳 UI。
- RingConfig 的正面/背面只保存 preset ID 顺序。
- JPG 质量和输出目录从 RingConfig 全局字段迁入 preset；迁移需要确定默认值。
- 每个 Job 保存不可变快照。

### 8.2 ConversionJob / ConversionFileResult

```text
ConversionJob
- id
- sources: immutable file snapshot
- preset: immutable snapshot
- state: Queued | Running | Completed | CompletedWithErrors | Failed | Cancelled
- files: [ConversionFileResult]
- created_at
- cancellation handle

ConversionFileResult
- source
- detected_format
- state: Pending | Running | Done | Skipped | Failed | Cancelled
- progress: 0...1, monotonic
- outputs: [path]
- skip_reason / error: structured code + detail
```

约束：结果总数必须能追溯到所有输入；特殊批处理若消费多个输入，也要保留输入到输出的关系。

### 8.3 ConverterEngine / ConversionEdge

```text
ConverterEngine
- id
- edges()
- status()
- max_parallelism()
- convert(step, progress, cancellation) -> outputs

ConversionEdge
- from
- to
- cost
- allowed modes / source restrictions

ConversionStep
- input
- from / to
- preset snapshot
- output allocator
- work directory
- original source
```

引擎不能直接读取 RingConfig，不能自行决定全局冲突策略，不能修改源文件。

### 8.4 OutputAllocator

```text
reserve(request) -> ReservedOutput | Skipped
commit(reservation, completed temporary output) -> final path
discard(reservation)
```

职责：

- 文件名 token、目录、重名和 Skip。
- 源路径保护和并行预留。
- 最终提交与失败清理。
- 支持未来一个输入产生多个输出。

### 8.5 JobQueue

职责：

- 创建 Job、注册取消句柄、调度文件和引擎。
- 逐文件 Detect → Plan → Execute → Finalize。
- 发出状态和进度事件。
- 管理 Job 工作目录生命周期。
- 圆环、设置页、未来其他入口共用同一服务。

---

## 9. 必须对齐与可适配部分

### A：必须对齐的核心语义

- Preset 不是目标格式字符串。
- Job 使用不可变 preset 快照。
- 同格式是否可执行由实际能力决定。
- 每个输入都有可解释结果，禁止静默忽略。
- 源文件默认保留，Overwrite 也不能覆盖源路径。
- 输出重名、失败清理、取消和并行预留是转换核心职责。
- 图像变换只应用一次；不支持保真处理时不得假装成功。
- 可用性判断与执行使用同一能力来源，避免“扇区可选但执行无路由”。

### B：可按 ClipBase 阶段适配

- 是否采用 Dijkstra，以及首版最大路由步数。
- Job 之间串行还是并行；文件并行度和每引擎上限。
- 临时目录具体位置、命名和清理周期。
- 错误文案、Toast、历史展示和进度动画。
- 首版支持的格式数量和 preset 参数数量。
- 是否接入 PDF、Office、FFmpeg、OCR 等外部引擎。

适配不能改变 A 中的数据安全和结果语义。

---

## 10. 验收测试清单

### P0 必须通过

- [ ] JPG → JPG 降质量在有 self edge 时成功，无 self edge 时明确跳过/失败，不再一律按“同格式”处理。
- [ ] 两个 JPG preset 分别使用不同质量和命名，互不读取全局质量。
- [ ] 任务启动后修改 preset，不影响该任务及其重试快照。
- [ ] 混合批次中不支持、无法识别、空文件均有逐文件结果，输入数量可核对。
- [ ] 两个同名来源并行输出不会互相覆盖。
- [ ] Rename、Overwrite、Skip 三种冲突都符合规则；Overwrite 不覆盖源文件。
- [ ] 编码中途失败或取消后，没有可被误认为成功结果的半成品，已有目标文件不被误删。
- [ ] 源文件转换前后内容和路径保持符合规则；普通转换绝不修改源。
- [ ] 动画输入在不支持保帧时有明确限制；支持时帧数、延迟和循环语义符合预期。
- [ ] EXIF 方向、ICC、元数据分别按 preset 验证，不依赖扩展名猜测。
- [ ] 透明 PNG/WebP → JPG 的背景色确定，无随机黑底或隐式不一致。
- [ ] 图片 → MD/HTML 的实际产物与动作名称一致，不把“引用源图”宣称为“嵌入/渲染”。

### P1 必须通过

- [ ] 运行中取消能停止后续步骤，状态为 Cancelled，工作文件被清理。
- [ ] 进度不倒退，步骤切换和异步回调乱序也不会倒退。
- [ ] 单引擎并发不超过上限，一个慢文件不会破坏其他文件结果。
- [ ] 多步路由的中间文件不进入最终输出列表。
- [ ] 旋转/翻转和动画速度在多步路由中只应用一次。
- [ ] 无路由、引擎缺失、显式跳过、用户取消使用不同结果原因。
- [ ] 圆环可用性和实际执行来自同一 Planner/能力目录。

---

## 11. 证据文件

### Filee

- `src/Filee.Core/Presets/Preset.cs`
- `src/Filee.Core/Presets/OutputRule.cs`
- `src/Filee.Core/Presets/OutputPathResolver.cs`
- `src/Filee.Core/Conversion/IConverter.cs`
- `src/Filee.Core/Conversion/ConverterCatalog.cs`
- `src/Filee.Core/Conversion/RoutePlanner.cs`
- `src/Filee.Core/Conversion/ConversionJob.cs`
- `src/Filee.Core/Conversion/JobQueue.cs`
- `src/Filee.App/Services/ConversionService.cs`
- `src/Filee.App/Services/PresetAvailability.cs`
- `src/Filee.Engines/EngineRegistry.cs`

### ClipBase

- `src-tauri/src/core/convert/mod.rs`
- `src-tauri/src/commands/convert.rs`
- `src-tauri/src/config/settings.rs`
- `src-tauri/src/core/ring/mod.rs`
- `src/lib/ipc.ts`
- `src/pages/RingSettings.tsx`
- `src/windows/RingWindow.tsx`

以上结论均来自这些文件的静态源码证据。真实引擎的保真度、性能、异常时序和 UI 表现仍需按验收清单运行验证。
