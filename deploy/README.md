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
