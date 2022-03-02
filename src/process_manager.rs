use std::process::{process, Command, Stdio};

//    let mut child = Command::new("julia")
//        .stdin(Stdio::piped())
//        .stdout(Stdio::piped())
//        .spawn()?;
//
//    let child_stdin = child.stdin.as_mut().unwrap();
//    let input: &str = r#"
//        # using DataFrames
//        # df = DataFrame()
//        # df.data  =[1,2,3]
//        show(df.data)
//
//
//        "#;
//    child_stdin.write_all(input.as_bytes())?;
//    // Close stdin to finish and avoid indefinite blocking
//    drop(child_stdin);
//
//    let output = child.wait_with_output()?;
//
//    println!("output = {:?}", output);
//
//    Ok(())
//}
