//! 环形转换命令：ring 窗口据此渲染，并把用户选择回流到会话执行层。
//!
//! 会话状态机在 `crate::core::ring`；这里只做「窗口 ↔ 状态机」的薄接线，
//! 保证拖动手势与环形盘共用同一执行逻辑（评审 D5 / 任务 12）。

use crate::core::option_tap::{self, AxRepair, GestureStatus};
use crate::core::ring::{RingManager, RingSnapshot};

/// ring 窗口挂载 / 收到更新事件时拉取当前会话快照。
#[tauri::command]
pub fn ring_snapshot() -> RingSnapshot {
    RingManager::snapshot()
}

/// 用户在环形盘选定一个可用扇区（动作 id = 预设 id）。
///
/// 禁用扇区的提交会被 `RingManager::execute` 的第二道闸拦下，不会启动 Job。
#[tauri::command]
pub fn ring_execute(action_id: String) {
    RingManager::execute(action_id);
}

/// 取消环形盘（松手落在中心/环外/禁用项、失焦）。
#[tauri::command]
pub fn ring_cancel() {
    RingManager::cancel();
}

/// 结果动画播完后收起环形盘（原地成功/失败反馈结束）。
#[tauri::command]
pub fn ring_dismiss() {
    RingManager::dismiss();
}

/// 手势现场状态：开关、辅助功能授权、监听是否真的起来、前台条件与最近一次
/// 「没反应」的原因。设置页轮询它，用户不用再去翻日志猜。
#[tauri::command]
pub fn ring_gesture_status() -> GestureStatus {
    option_tap::status()
}

/// 修复辅助功能授权：ad-hoc 签名重新构建后，系统会留着一条指向旧 cdhash 的
/// 失效记录（界面显示已勾选，实际不生效）。这里先 `tccutil reset` 清掉，再带
/// prompt 重新登记，用户只需在系统设置里勾一次。
#[tauri::command]
pub fn ring_gesture_repair(app: tauri::AppHandle) -> AxRepair {
    option_tap::repair(&app.config().identifier)
}
