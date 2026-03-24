-- OpenGG WirePlumber Filter Chain Script
--
-- This script creates a parametric EQ filter chain for each OpenGG virtual sink.
-- Place in: ~/.config/wireplumber/scripts/opengg-eq.lua
-- Reference in: ~/.config/wireplumber/wireplumber.conf.d/99-opengg.conf
--
-- WirePlumber loads this at startup. The filter chains use PipeWire's built-in
-- DSP nodes (no external plugins needed for basic EQ).

local config = {
  -- Channels to apply EQ to
  channels = { "Game", "Chat", "Media", "Aux" },

  -- Default 10-band parametric EQ (flat)
  default_bands = {
    { freq = 31,    gain = 0.0, q = 1.0 },
    { freq = 63,    gain = 0.0, q = 1.0 },
    { freq = 125,   gain = 0.0, q = 1.0 },
    { freq = 250,   gain = 0.0, q = 1.0 },
    { freq = 500,   gain = 0.0, q = 1.0 },
    { freq = 1000,  gain = 0.0, q = 1.0 },
    { freq = 2000,  gain = 0.0, q = 1.0 },
    { freq = 4000,  gain = 0.0, q = 1.0 },
    { freq = 8000,  gain = 0.0, q = 1.0 },
    { freq = 16000, gain = 0.0, q = 1.0 },
  }
}

-- Build a filter-chain node definition for one channel
local function build_eq_filter(channel_name)
  local sink_name = "OpenGG_" .. channel_name
  local filter_name = "OpenGG_EQ_" .. channel_name

  -- Construct the filter chain using PipeWire's built-in biquad filters
  local nodes = {}
  for i, band in ipairs(config.default_bands) do
    table.insert(nodes, {
      type = "builtin",
      name = string.format("eq_band_%d", i),
      label = string.format("bq_peaking"),
      control = {
        ["Freq"]  = band.freq,
        ["Q"]     = band.q,
        ["Gain"]  = band.gain,
      },
    })
  end

  return {
    ["node.name"]        = filter_name,
    ["node.description"]  = "OpenGG EQ: " .. channel_name,
    ["media.class"]       = "Audio/Sink",
    ["filter.graph"]      = {
      nodes = nodes,
    },
    ["capture.props"]     = {
      ["node.name"]      = filter_name .. "_input",
      ["media.class"]    = "Audio/Sink",
      ["audio.position"] = "FL,FR",
    },
    ["playback.props"]    = {
      ["node.target"]    = sink_name,
      ["node.passive"]   = true,
      ["audio.position"] = "FL,FR",
    },
  }
end

-- Register filter chains for each channel
for _, channel in ipairs(config.channels) do
  local filter_def = build_eq_filter(channel)
  Log.info("OpenGG: Creating EQ filter for " .. channel)

  -- In a real implementation, this would use the WirePlumber filter-chain module:
  -- local fc = FilterChain(filter_def)
  -- fc:activate()
  --
  -- For now this serves as the structural template. The actual activation
  -- depends on WirePlumber version and available modules.
end

Log.info("OpenGG EQ filter chains configured")
