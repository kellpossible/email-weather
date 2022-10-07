# Email Weather [![github actions badge](https://github.com/kellpossible/email-weather/actions/workflows/rust.yml/badge.svg)](https://github.com/kellpossible/email-weather/actions/workflows/rust.yml) [![github actions scheduled audit badge](https://github.com/kellpossible/email-weather/actions/workflows/scheduled-audit.yml/badge.svg)](https://github.com/kellpossible/email-weather/actions/workflows/scheduled-audit.yml) [![dependency status](https://deps.rs/repo/github/kellpossible/email-weather/status.svg)](https://deps.rs/repo/github/kellpossible/email-weather)

## Idea

A service that:

+ Recieve an email containing requested coordinates, and optionally elevation.
+ Return an email with a text description 160 characters with the forecast.
+ Ideally use the ECMWF forecast model
  + https://confluence.ecmwf.int/display/DAC/ECMWF+open+data%3A+real-time+forecasts#ECMWFopendata:realtimeforecasts-Locations
  + https://github.com/ecmwf/ecmwf-opendata
  + https://github.com/open-meteo/open-meteo 
+ Compensate reported temperature using altitude
  + https://www.opentopodata.org/datasets/aster/ Global 30m resolution
  + https://www.opentopodata.org/datasets/nzdem/ NZ 8m resolution (probably want to use another API to determine whether we are inside New Zealand? Or check inside a bounding box first)
+ Use a persistent queue for message processing robustness
  + https://github.com/tokahuke/yaque
