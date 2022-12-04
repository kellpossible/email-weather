use open_meteo::WeatherCode;

pub fn main() {
    println!("<table>");
    print!("<tr>");
    print!("<th>Code</th><th></th>");
    print!("</tr>");
    for variant in WeatherCode::enumerate() {
        let code = variant.code();
        println!("<tr><td>{code}</td><td>{variant}</td></tr>")
    }
    println!("</table>");
}
