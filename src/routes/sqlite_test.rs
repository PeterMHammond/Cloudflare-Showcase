use worker::*;
use crate::utils::middleware::ValidationState;

#[derive(Debug, serde::Serialize)]
struct TestResult {
    test_name: String,
    passed: bool,
    message: String,
}

pub async fn handle(
    _req: Request,
    ctx: RouteContext<ValidationState>,
) -> Result<Response> {
    console_log!("SQLite test handler called");
    let mut test_results = Vec::new();

    // Test 1: Access Storage from route context
    let test_1 = match ctx.env.durable_object("ExampleSqliteDO") {
        Ok(namespace) => {
            match namespace.id_from_name("sqlite-demo-instance") {
                Ok(id) => {
                    match id.get_stub() {
                        Ok(stub) => {
                            // Try to run SQL test via the DO
                            console_log!("Attempting to fetch from DO with path /sqlite/api/sql-test");
                            match stub.fetch_with_str("/sqlite/api/sql-test").await {
                                Ok(mut response) => {
                                    match response.text().await {
                                        Ok(text) => TestResult {
                                            test_name: "SQL Access Test via DO".to_string(),
                                            passed: !text.contains("error"),
                                            message: text,
                                        },
                                        Err(e) => TestResult {
                                            test_name: "SQL Access Test via DO".to_string(),
                                            passed: false,
                                            message: format!("Failed to read response: {}", e),
                                        }
                                    }
                                },
                                Err(e) => TestResult {
                                    test_name: "SQL Access Test via DO".to_string(),
                                    passed: false,
                                    message: format!("Failed to fetch from DO: {}", e),
                                }
                            }
                        },
                        Err(e) => TestResult {
                            test_name: "SQL Access Test via DO".to_string(),
                            passed: false,
                            message: format!("Failed to get stub: {}", e),
                        }
                    }
                },
                Err(e) => TestResult {
                    test_name: "SQL Access Test via DO".to_string(),
                    passed: false,
                    message: format!("Failed to get ID: {}", e),
                }
            }
        },
        Err(e) => TestResult {
            test_name: "SQL Access Test via DO".to_string(),
            passed: false,
            message: format!("Failed to get DO namespace: {}", e),
        }
    };
    test_results.push(test_1);

    // Create response with test results
    let html = format!(
        r#"
        <h1>SQLite Implementation Tests</h1>
        <table border="1">
            <tr>
                <th>Test Name</th>
                <th>Status</th>
                <th>Message</th>
            </tr>
            {}
        </table>
        <p><a href="/sqlite">Back to SQLite Demo</a></p>
        "#,
        test_results.iter()
            .map(|result| format!(
                "<tr><td>{}</td><td style='color: {}'>{}</td><td>{}</td></tr>",
                result.test_name,
                if result.passed { "green" } else { "red" },
                if result.passed { "PASSED" } else { "FAILED" },
                result.message
            ))
            .collect::<Vec<_>>()
            .join("\n")
    );

    Response::from_html(html)
}