# prom2jsonrs
A (very) simple utility to parse promethues data as json. Insipred from the Golang tool [prom2json](https://github.com/prometheus/prom2json)

## Example Usage
```
prom2jsonrs http://localhost:9090/metrics  | jq
```

## TODO's
* Better error handling
* Add support for specifying options for the http(s) request
