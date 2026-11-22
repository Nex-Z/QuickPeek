# 预览器扩展边界 · protocol 1

首版预留协议，不扫描或执行第三方插件，也没有插件商店。
公共 Rust 数据类型位于 `quickpeek::core::plugin`，解析层可以独立于 WinUI 使用。

## 计划中的进程边界

```text
QuickPeek shell → Viewer registry → Built-in viewers
                                → plugin-host.exe → third-party process
```

插件必须是独立进程，不加载第三方 DLL，不跨模块传 Rust trait object 或 Rust ABI。
宿主通过独立命名管道传输 UTF-8 JSON，4 字节小端长度前缀，每条消息最大 1 MiB。
握手核对 protocol_version；请求 id 用于匹配响应和取消过期工作。
UI 只消费受支持的文本或图片结果，未来扩展类型需要升级协议。

## 预留清单

```json
{
  "id": "example.preview",
  "protocol_version": 1,
  "executable": "viewer.exe",
  "extensions": ["example"]
}
```

请求：`probe`、`preview`（含 max_bytes）、`cancel`。
响应：`text`、`image`（本地 PNG）、`error`。

启用插件前还需实现：用户启用管理、清单和路径校验、宿主管道 ACL、启动握手、
超时与取消、子进程资源限制、崩溃恢复、输出文件所有权和清理、扩展优先级。
以上是明确预留的后续接口，并非首版已经完成的隔离保证。
