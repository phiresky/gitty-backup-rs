unicode testing

```rust
    let p: serde_json::Value = serde_json::from_str(r#" "\u0000\u0001\u0002\u0003\u0004\u0005\u0006\u0007\b\t\n\u000b\f\r\u000e\u000f\u0010\u0011\u0012\u0013\u0014\u0015\u0016\u0017\u0018\u0019\u001a\u001b\u001c\u001d\u001e\u001f !\" #$%&'()*+,-./0123456789:;<=>?@ABCDEFGHIJKLMNOPQRSTUVWXYZ[\\]^_`abcdefghijklmnopqrstuvwxyz{|}~ ¡¢£¤¥¦§¨©ª«¬­®¯°±²³´µ¶·¸¹º»¼½¾¿ÀÁÂÃÄÅÆÇÈÉÊËÌÍÎÏÐÑÒÓÔÕÖ×ØÙÚÛÜÝÞßàáâãäåæçèéêëìíîïðñòóôõö÷øùúûüýþÿ" "#).unwrap();
    //let s = "\"/mojibake/français/path\"";
    //let b = b"/mojibake/fran\xe7ais/path";
    // let p: serde_json::Value = serde_json::from_str(s).unwrap();
    // println!("{:?} {}", String::from_utf8(b.to_vec()), s);
    let s = "\u{e7}";
    println!("should be E7: {}", s);
    if let serde_json::Value::String(x) = p {
        println!("{:?}", x);
    }
```