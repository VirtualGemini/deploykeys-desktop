# Phase 2: GitHub 集成与核心功能实现

## 目标

实现 GitHub OAuth 认证和核心 API 集成，建立完整的密钥生命周期管理。

---

## 任务清单

### 2.1 GitHub OAuth Device Flow ⏳
- [ ] 实现 Device Flow 认证流程
- [ ] OAuth token 管理（存储/刷新）
- [ ] 用户信息获取

### 2.2 GitHub API 集成 ⏳
- [ ] Deploy Key CRUD 操作
- [ ] Repository 列表获取
- [ ] Installation 管理

### 2.3 核心业务流程 ⏳
- [ ] 完整的 Key 绑定流程
- [ ] Key 验证和状态同步
- [ ] 错误恢复机制

### 2.4 GUI 实现 ⏳
- [ ] 登录界面
- [ ] Repository 列表界面
- [ ] Key 管理界面
- [ ] 状态显示

---

## 实现策略

**优先级**: P0 (关键路径)
**时间估计**: 4-6 小时
**验证方式**: 端到端手动测试

---

## 技术要点

### GitHub Device Flow
```
1. 请求 device code
2. 显示 user code + 验证 URL
3. 轮询 access token
4. 存储到 Keyring
```

### API 集成重点
- Rate limiting 处理
- 错误重试机制
- Token 自动刷新
- 请求日志脱敏

### 状态管理
```
Pending → Activating → Active
         ↓
       Failed ← (重试)
```

---

## 开始实现

**当前任务**: 2.1.1 - GitHub OAuth Device Flow 基础实现
