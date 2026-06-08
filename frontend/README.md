# MarketBridge Frontend

Static funding monitor for a running MarketBridge API.

Run the API:

```bash
cd /Users/w0x7ce/Downloads/AACC/MarketBridge
MARKETBRIDGE_CONFIG=./config.min.yaml cargo run
```

Serve the frontend:

```bash
cd /Users/w0x7ce/Downloads/AACC/MarketBridge/frontend
python3 -m http.server 8090
```

Open:

```text
http://127.0.0.1:8090
```

For a VPS API:

```text
http://127.0.0.1:8090/?api=http://YOUR_VPS_IP:8080
```
