+++
title = "Email Weather Service"
sort_by = "weight"
+++

# Disclaimer and License

This experimental service is provided free to community, and the [contributors to this project](https://github.com/kellpossible/email-weather/graphs/contributors) do not accept any legal responsibility for the quality of the forecasts that are provided, or the quality of service on offer. 

<b>Use this service/software at your own risk, and at your own expense</b>, there may be unexpected innacuracies, bugs or service interruptions. Some satellite communication services charge per message, care has been taken to reduce the number and length of messages, and to prevent accidental overuse, however the contributors accept no responsibility for unexpected expenses resulting from the use of this service/software.

It is highly recommended to cross-check forecasts provided by this service with other sources of information to help make the most informed judgement about weather conditions in your area. Weather data is currently provided by <a href="https://open-meteo.com/">Open-Meteo.com</a> under the <a href="https://creativecommons.org/licenses/by-nc/4.0/">CC BY-NC 4.0</a> license. This software is open source (available [on GitHub](https://github.com/kellpossible/email-weather)) and licensed under the [MIT Software License](https://github.com/kellpossible/email-weather/blob/main/LICENSE).

# How Use Email Weather

Using the Email Weather Service is simple, you just need to send an email to{{ service_email() }}. See [Email Body](#email-body) section for what information you will need to include in the body of your email in order to obtain a forecast.

# Email Subject

The subject of the email can be anything, it can be helpful to use a name for which you are retrieving the forecast.

# Email Body

The contents of the email body depends on which service you are using to send your email.

## Standard Email

For any standard email account, you will need to at least provide the requested [position](#position) in the body of your email as part of the [forecast request](#forecast-request). For example to obtain the forecast for [London](https://goo.gl/maps/sUFSPJQ6ByW4y5os6), you can enter the following text in your email body, which is requested forecast position in `latitude,longitude` format:

{% email_block(subject="Forecast for London") %}
51.5287718,-0.2416804
{% end %}

See [Forecast Request](#forecast-request) section for more information on what you can request in a forecast.

## InReach

If you are sending an email from an [InReach communication device](https://discover.garmin.com/en-US/inreach/personal/), and you elect to wait for GPS signal before sending the message, you do not need to include the [position](#position) in the [forecast request](#forecast-request), this service will use the position of your device reported at the time the message was sent to obtain the forecast for your location. However, if you want to obtain the forecast for a different location, you can include the [position](#position) in the [forecast request](#forecast-request).



# Forecast Request

## Position
