# VPS deployment

This example assumes the project is deployed to:

```text
/opt/marketbridge
```

Build the server:

```bash
cd /opt/marketbridge
cargo build --release
```

Install the systemd unit:

```bash
sudo cp deploy/marketbridge.service.example /etc/systemd/system/marketbridge.service
sudo systemctl daemon-reload
sudo systemctl enable --now marketbridge
sudo systemctl status marketbridge
```

Check the API:

```bash
curl -s http://127.0.0.1:8080/health
curl -s "http://127.0.0.1:8080/v1/market/perpetual-funding?exchange=binance&quote=USDT&limit=50000" \
  | jq '.funding | map(select(.funding_rate_pct < -0.2))'
```

Optional Nginx frontend and API proxy:

```bash
sudo cp deploy/nginx-marketbridge.conf.example /etc/nginx/sites-available/marketbridge
sudo ln -sf /etc/nginx/sites-available/marketbridge /etc/nginx/sites-enabled/marketbridge
sudo nginx -t
sudo systemctl reload nginx
```

Then open:

```text
http://YOUR_SERVER_NAME/
```
# Deployment examples

`marketbridge.service.example` keeps the Rust data/API plane alive. The
companion `marketbridge-binance-short-opportunity.service.example` keeps the
Python research scanner alive and restarts it if the process exits. Both are
read-only research services; neither contains an order or wallet path.

For systemd, copy both files to `/etc/systemd/system/`, replace `/opt/marketbridge`
with the checkout path and `.venv/bin/python3` with the Python environment, then:

```bash
sudo systemctl daemon-reload
sudo systemctl enable --now marketbridge.service
sudo systemctl enable --now marketbridge-binance-short-opportunity.service
sudo journalctl -u marketbridge-binance-short-opportunity.service -f
```

On macOS, run the same two commands in separate Terminal tabs or wrap them in
launchd/`nohup`; the process must keep the Rust service running before the
scanner can query `127.0.0.1:8080`.

For a broad but explicit Binance universe, use
`scripts/export_binance_perp_universe.py` against a running local API, review
the generated `work/binance-perp-symbols.yaml`, paste it into the local config,
and restart MarketBridge. This avoids silently subscribing to every newly
listed contract.
