# Weather observations

> Deterministic Open-Meteo observations used as explicit inputs to event and
> market-calibration research.

## Cases

- `weather_event_observer.py`: normalized daily observation/forecast bucket.
- `weather_pressure_differential.py`: weather update versus a matching public
  market quote; investigation candidate only.
- `weather_market_calibration.py`: caller-owned JSONL manifest joined with
  verified closed-market outcomes.

Weather identity, location, timezone, observation time, forecast revision,
market identity and resolution rule are caller responsibilities. The examples
do not infer probabilities from weather alone.

See [`../README.md`](../README.md) for commands and [`README.md`](README.md)
for the original bilingual overview.

## Boundary

No weather derivatives or prediction-market orders are placed; outputs are
deterministic observations and calibration evidence only.
