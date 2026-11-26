# Running and using parth_database

## Build and test

    cd engine

    cargo build
    cargo tes
t

## S

tart the server



Set a secret token of at least 16 characters. The server refuses to start without it. Pick your own value and keep it out of the repository.

    $env:PARTH_DB_DIR 

= "D:\parth_data"
    $e
nv:PARTH_DB_TOKEN = "<your secret, 16+ characters>"
 
   $env:PARTH_DB_ADDR = "127.0.0.1:8787"
    cargo r
un

P

ARTH_DB_DIR is where the database files live. PARTH_DB_ADDR defaults to 127.0.0.1:8787. Non-loopback addresses are refused unless PARTH_DB_ALLOW_REMOTE=1 is set. There is no TLS, so do not expose the port to a network.


## Use the frontend

Serve


 the repository root with any static file server, for example `python -m http.server 8000 --bind 127.0.0.1`, and open http://127.0.0.1:8000/. Enter the server address and the token, then press Connect. Click a table to see its rows. Click a truncated cell, or focus it with Tab and press Enter or Space, to show its full value. If the server stops answering, the page keeps the last known data, dims it and marks it stale until a request succeeds again.



## Use the API directly


Every endpoint except /api/he
alth needs the header `Authorization: Bearer <token>`.

  

  Invoke-RestMethod http://127.0.0.1:8787/api/tables -Headers @{Authorization="Bearer <token>"}


## S
QL

    CR

EATE TABLE notes (id INT PRIMARY KEY, body TEXT)
  
  INSERT INTO notes VALUES (1, 'hello')

    SELECT * FROM notes WHERE id = 1 ORDER BY id LIMIT 10
    UP
DATE notes SET body = 'changed' WHERE id = 1
    
DELETE FROM notes WHERE id = 1
    D
ROP TABLE notes

Column 

types are INT, REAL, TEXT and BOOL. Every table needs exactly one PRIMARY KEY column. A row must fit in 1000 bytes. There are no transactions, JOINs or GROUP BY.
