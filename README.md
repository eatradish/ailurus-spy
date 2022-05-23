# ailurus-spy
小熊貓監視器！ - A bilibili dynamic and live status checker written in rust!

[Telegram channel](https://t.me/ailuluailurus)

## To-Do!
- [x] 直播检查器
- [x] 动态检查器
- [ ] 微博动态检查器（因为我家小熊猫要开微博啦！）
- [x] Telegram 转发支持
- [ ] 多接收端支持（以插件形式）
- [x] 单直播 + 动态检查器
- [ ] 多直播 + 动态检查器（需要写调度器调度任务，防止被防爬）
- [ ] 清晰的配置文件（可添加任意多直播和动态监测、设置查询间隔、设置调度参数等）

## 使用

1. 安装依赖：
    在 AOSC OS 上：
    ```
    sudo apt install redis rustc
    ```
2. 编译：

    ```
    cargo build --release
    ```
3. 往 `.env` 写入需要的配置

    ```
    # Telegram 机器人 token：
    TELOXIDE_TOKEN="$TOKEN"
    # 代理
    http_proxy="http://127.0.0.1:8118"
    https_proxy="http://127.0.0.1:8118"
    # 订阅 B 站动态用户的 uid：
    AILURUS_DYNAMIC="1501380958"
    # 订阅 B 站用户直播间的开播状态：
    AILURUS_LIVE="22746343"
    # Telegram chat id (群组、频道、私聊)
    AILURUS_CHATID="-1001675012012"
    ```

4. 运行：

    ```
    # 先启动 redis 服务器
    redis-server
    # 启动主程序
    ./target/release/ailurus-spy
    ```

## Ref
- 小熊猫 -> [小熊猫](https://space.bilibili.com/1501380958/) 
