+++
title = "User Manual"
description = "Email Weather Service User Manual"
template = "print.html"
in_search_index = false
weight = 0
+++

{{ load_snippet(path="snippets/disclaimer_license.md") }}

# How Use Email Weather

Using the Email Weather Service is simple, you just need to send an email to{{ service_email() }}. See [Email Body](#email-body) section for what information you will need to include in the body of your email in order to obtain a forecast.

# Email Subject

The subject of the email can be anything, it can be helpful to use a name for which you are retrieving the forecast.

# Email Body

The contents of the email body depends on which service you are using to send your email.

## Standard Email

For any standard email account, you will need to at least provide the requested [position](#position) in the body of your email as part of the [forecast request](#forecast-request). For example to obtain the forecast for [London](https://goo.gl/maps/sUFSPJQ6ByW4y5os6), you can enter the following text in your email body, which is requested forecast position in `latitude,longitude` format:

{% new_email(subject="Forecast for London") %}
51.5287718,-0.2416804
{% end %}

You can then expect to receive a response similar to:

{{ response_email(body_path="snippets/london_short_body.html") }}

See [Forecast Request](#forecast-request) section for more information on what you can request in a forecast.

## InReach

If you are sending an email from an [InReach communication device](https://discover.garmin.com/en-US/inreach/personal/), and you elect to wait for GPS signal before sending the message, you do not need to include the [position](#position) in the [forecast request](#forecast-request), this service will use the position of your device reported at the time the message was sent to obtain the forecast for your location. However, if you want to obtain the forecast for a different location, you can include the [position](#position) in the [forecast request](#forecast-request).

### Limitations

Using the InReach with this service currently has the following limitations:

+ A maximum of 160 characters in the reply message.
+ Only the [short format](#short) is supported (due to the above limitation).


# Forecast Request

The forecast request is specified in the body of email that you send to {{ service_email() }} with a specific syntax which is described in the subsequent sections of this document. Please ensure that you use only plain text, don't apply formatting or HTML email signatures to your mail if possible to ensure maximum compatibility with the service.

# Position

Position for the requested forecast is specified using `latitude,longitude` format.

{% new_email() %}
<b>51.5287718,-0.2416804</b>
{% end %}

The position always needs to be in the first position of your request. For instance, here is a position in combination with a [Format Detail](#format).

{% new_email() %}
<b>51.5287718,-0.2416804</b> ML
{% end %}
<br>

# Format

There are many options available for you to customise the format of the forecast message you will receive.
The format specification is specified preceding with an `M`, followed by the specification. The default format is [`MS` Short](#short).

## Short

Short format (`MS`) produces a forecast message which is extremely short, optimised for use with satellite communciation devices like the [InReach](#inreach).

{% new_email() %}
51.5287718,-0.2416804 <b>MS</b>
{% end %}
{{ response_email(body_path="snippets/london_short_body.html") }}

While it may appear cryptic at first, the response is fairly easy to understand.

The first line takes the format:

<table>
<tr>
<th>Timezone</th>
<th>Forecast Elevation (meters/100)</th>
<th>Terrain Elevation (meters/100)</th>
</tr>
<td>TzGMT</td><td>FE33</td><td>TE34</td>
<tr>
</tr>
</table>

Subsequent lines which form the forecast take the format:

<table>
<tr>
<th>Time (day T hour)</th>
<th>WMO Weather Code</th>
<th>Freezing Level (meters/100)</th>
<th>Wind (speed/10 @ direction/10)</th>
<th>Precipitation (mm cummulative since previous entry)</th>
</tr>
<tr>
<td>04T03</td><td>C3</td><td>F7</td><td>W1@3</td><td>P0</td>
</tr>
<tr>
<td>04T09</td><td>C53</td><td>F6</td><td>W1@9</td><td>P1</td>
</tr>
</table>

The [WMO 4677 Present Weather Code](https://www.nodc.noaa.gov/archive/arc0021/0002199/1.1/data/0-data/HTML/WMO-CODE/WMO4677.HTM) is a 1 or 2 digit number representing the state of the weather:

{{ load_snippet(path="snippets/wmo_codes.html", html=true) }}

## Long
