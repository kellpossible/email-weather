+++
title = "Email Weather Service"
sort_by = "weight"
+++

{{ load_snippet(path="snippets/disclaimer_license.md") }}

# What is Email Weather Service?

Email Weather Service is a plain-text weather forecasting service via email. Send an email to {{ service_email() }} specifying your requested forecast location and parameters, and receive a reply containg the forecast.

For example, here is an email requesting the forecast for [London](https://goo.gl/maps/sUFSPJQ6ByW4y5os6):

{% new_email(subject="Forecast for London") %}
51.5287718,-0.2416804
{% end %}

You can then expect to receive a response similar to:

{{ response_email(body_path="snippets/london_short_body.html") }}

See the [User Manual](./manual#short) for help understanding the short forecast format. 

You can also request a more detailed and easier to read plain text format of forecast email:

{% new_email(subject="Forecast for London") %}
51.5287718,-0.2416804 <b>ML</b>
{% end %}

{{ response_email(body_path="snippets/london_long_plain_body.html") }}

Or even a HTML version:

{% new_email(subject="Forecast for London") %}
51.5287718,-0.2416804 <b>MLH</b>
{% end %}
{{ response_email(body_path="snippets/london_html_body.html") }}

# User Manual

See the [User Manual](./manual) for a detailed description of how to interact with this service via email.
