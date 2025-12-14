POST http://localhost:8080/api/v1/process/jar

[Multipart]
file: file,test/assets/test.jar;

[Query]
username: daysling
timestamp: 170000000000000
nonce: random_string_here!!

[Options]
compressed: true
output: test/assets/test.out.jar
