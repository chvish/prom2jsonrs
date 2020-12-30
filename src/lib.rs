use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate maplit;

lazy_static! {
    static ref METRIC_REGEX_NO_LABEL: Regex =
        Regex::new(r"([a-zA-Z_:][a-zA-Z0-9_:]*)\s(-?[\d.]+(?:e-?\d+)?|NaN)").unwrap();
    static ref METRIC_REGEX_WITH_LABEL: Regex =
        Regex::new(r"[a-zA-Z_:][a-zA-Z0-9_:]*\{(.*)\}\s(-?[\d.]+(?:e-?\d+)?|NaN)").unwrap();
    static ref LABELS_REGEX: Regex = Regex::new("([a-zA-Z0-9_:]*)=\"([^\"]+)\"").unwrap();
}

type Labels = HashMap<String, String>;
type Value = String;

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Metric {
    labels: Option<Labels>,
    value: Value,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Summary {
    labels: Option<Labels>,
    quantiles: Labels,
    count: Value,
    sum: Value,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Histogram {
    labels: Option<HashMap<String, String>>,
    buckets: Labels,
    count: Value,
    sum: Value,
}

#[derive(Debug, PartialEq, Serialize)]
enum MetricType {
    Gauge,
    Histogram,
    Summary,
}

#[derive(Serialize)]
struct MetricFamily {
    metric_type: MetricType,
    metric_name: String,
    help: String,
    data: Vec<Box<dyn MetricLike>>,
}

#[derive(Serialize)]
pub struct PrometheusData {
    metrics: Vec<MetricFamily>,
}

#[typetag::serde(tag = "type")]
trait MetricLike {
    fn parse_from_string(s: &str) -> (Value, Option<Labels>)
    where
        Self: Sized,
    {
        if let Some(caps) = METRIC_REGEX_NO_LABEL.captures(s) {
            (caps[2].to_string(), None)
        } else if let Some(caps) = METRIC_REGEX_WITH_LABEL.captures(s) {
            let value = caps[2].to_string();
            let mut labels: HashMap<String, String> = HashMap::new();
            for cap in LABELS_REGEX.captures_iter(&caps[1]) {
                labels.insert(cap[1].to_string(), cap[2].to_string());
            }
            (value, Some(labels))
        } else {
            panic!("Invalid format")
        }
    }

    fn metric_type() -> String
    where
        Self: Sized;
}

impl Metric {
    fn from_string(s: &str) -> Metric {
        let (value, labels) = Self::parse_from_string(s);
        Metric {
            labels: labels,
            value: value,
        }
    }
}

#[typetag::serde]
impl MetricLike for Metric {
    fn metric_type() -> String {
        String::from("DEFAULT")
    }
}

impl Summary {
    fn from_raw(metric_name: &str, raw_lines: &Vec<&str>) -> Summary {
        let mut sum = String::from("");
        let mut count = String::from("");
        let sum_prefix = format!("{}_sum", metric_name);
        let count_prefix = format!("{}_count", metric_name);
        let mut labels = HashMap::new();
        let mut quantiles = HashMap::new();
        for raw_line in raw_lines {
            if raw_line.starts_with(&sum_prefix) {
                sum = Summary::parse_from_string(raw_line).0;
            } else if raw_line.starts_with(&count_prefix) {
                count = Summary::parse_from_string(raw_line).0;
            } else if let Some(caps) = METRIC_REGEX_WITH_LABEL.captures(raw_line) {
                for cap in LABELS_REGEX.captures_iter(&caps[1]) {
                    let key = &cap[1];
                    let value = &cap[2];
                    match key {
                        "quantile" => quantiles.insert(key.to_string(), value.to_string()),
                        _ => labels.insert(key.to_string(), value.to_string()),
                    };
                }
            } else {
                panic!("Invalid format {}", raw_line)
            }
        }
        Summary {
            sum: sum,
            count: count,
            labels: Some(labels),
            quantiles: quantiles,
        }
    }
}

#[typetag::serde]
impl MetricLike for Summary {
    fn metric_type() -> String {
        String::from("SUMMARY")
    }
}

impl Histogram {
    fn from_raw(metric_name: &str, raw_lines: &Vec<&str>) -> Histogram {
        let mut sum = String::from("");
        let mut count = String::from("");
        let sum_prefix = format!("{}_sum", metric_name);
        let count_prefix = format!("{}_count", metric_name);
        let mut labels: HashMap<String, String> = HashMap::new();
        let mut buckets: HashMap<String, String> = HashMap::new();
        for raw_line in raw_lines {
            if raw_line.starts_with(&sum_prefix) {
                sum = Summary::parse_from_string(raw_line).0;
            } else if raw_line.starts_with(&count_prefix) {
                count = Summary::parse_from_string(raw_line).0;
            } else if let Some(caps) = METRIC_REGEX_WITH_LABEL.captures(raw_line) {
                for cap in LABELS_REGEX.captures_iter(&caps[1]) {
                    let key = &cap[1];
                    let value = &cap[2];
                    match key {
                        "le" => buckets.insert(value.to_string(), caps[2].to_string()),
                        _ => labels.insert(key.to_string(), value.to_string()),
                    };
                }
            } else {
                panic!("Invalid format {}", raw_line)
            }
        }
        Histogram {
            sum: sum,
            count: count,
            labels: Some(labels),
            buckets: buckets,
        }
    }
}

#[typetag::serde]
impl MetricLike for Histogram {
    fn metric_type() -> String {
        String::from("HISTOGRAM")
    }
}

impl MetricFamily {
    fn from_raw(raw: &Vec<&str>) -> MetricFamily {
        let mut raw_iter = raw.iter();
        let help = MetricFamily::metric_help_fron_raw(raw_iter.next().expect("invalid format"));
        let (metric_name, metric_type) =
            MetricFamily::metric_name_and_type(raw_iter.next().expect("invalid format"));
        let mut data: Vec<Box<dyn MetricLike>> = Vec::new();
        match metric_type {
            MetricType::Gauge => {
                for raw_line in raw_iter {
                    data.push(Box::new(Metric::from_string(raw_line)))
                }
            }
            MetricType::Histogram => {
                let count_prefix = format!("{}_count", metric_name);
                let mut histogram_lines: Vec<&str> = Vec::new();
                for raw_line in raw_iter {
                    histogram_lines.push(raw_line);
                    if raw_line.starts_with(&count_prefix) {
                        data.push(Box::new(Histogram::from_raw(
                            &metric_name,
                            &histogram_lines,
                        )));
                        histogram_lines = Vec::new();
                    }
                }
            }
            MetricType::Summary => {
                let count_prefix = format!("{}_count", metric_name);
                let mut summary_lines: Vec<&str> = Vec::new();
                for raw_line in raw_iter {
                    summary_lines.push(raw_line);
                    if raw_line.starts_with(&count_prefix) {
                        data.push(Box::new(Summary::from_raw(&metric_name, &summary_lines)));
                        summary_lines = Vec::new();
                    }
                }
            }
        }
        MetricFamily {
            metric_type: metric_type,
            metric_name: metric_name,
            help: help,
            data: data,
        }
    }

    fn metric_name_and_type(type_line: &str) -> (String, MetricType) {
        let tags: Vec<&str> = type_line.split_whitespace().collect();
        let (name, type_raw) = (tags[2], tags[3]);
        let metric_type = match type_raw {
            "gauge" => MetricType::Gauge,
            "counter" => MetricType::Gauge,
            "histogram" => MetricType::Histogram,
            "summary" => MetricType::Summary,
            unknown_metric => panic!("Unknown metric type {}", unknown_metric),
        };

        (name.to_string(), metric_type)
    }

    fn metric_help_fron_raw(help_line: &str) -> String {
        let tags: Vec<&str> = help_line.split_whitespace().collect();
        tags[3..].join(" ").to_string()
    }
}

impl PrometheusData {
    pub fn from_string(s: &str) -> PrometheusData {
        let mut metrics = Vec::new();
        let mut metric_lines = Vec::new();
        let mut num_comment_lines = 0;
        for line in s.lines() {
            if line.starts_with('#') {
                if num_comment_lines == 2 {
                    // One set complete
                    metrics.push(MetricFamily::from_raw(&metric_lines));
                    metric_lines = vec![line];
                    num_comment_lines = 1;
                } else {
                    num_comment_lines += 1;
                    metric_lines.push(line);
                }
            } else {
                metric_lines.push(line)
            }
        }
        PrometheusData { metrics: metrics }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn metric_parsing_works() {
        assert_eq!(
            Metric {
                labels: None,
                value: String::from("205632")
            },
            Metric::from_string("go_memstats_mspan_inuse_bytes 205632")
        );
        assert_eq!(
            Metric {
                labels: Some(hashmap!{
                    "dialer_name".to_string() => "default".to_string(),
                    "reason".to_string() => "unknown".to_string(),
                }),
                value: String::from("0")
            },
            Metric::from_string("net_conntrack_dialer_conn_failed_total{dialer_name=\"default\",reason=\"unknown\"} 0")
        )
    }

    #[test]
    fn raw_data_parsing_works() {
        let raw_data = "# HELP go_goroutines Number of goroutines that currently exist.
# TYPE go_goroutines gauge
go_goroutines 31
# HELP go_info Information about the Go environment.
# TYPE go_info gauge
go_info{version=\"go1.15.5\"} 1";
        let prom_data = PrometheusData::from_string(raw_data);
        assert_eq!(MetricType::Gauge, prom_data.metrics[0].metric_type)
    }

    #[test]
    fn summary_parsing_works() {
        let raw_data =
            "prometheus_engine_query_duration_seconds{slice=\"inner_eval\",quantile=\"0.5\"} NaN
prometheus_engine_query_duration_seconds{slice=\"inner_eval\",quantile=\"0.9\"} NaN
prometheus_engine_query_duration_seconds{slice=\"inner_eval\",quantile=\"0.99\"} NaN
prometheus_engine_query_duration_seconds_sum{slice=\"inner_eval\"} 12
prometheus_engine_query_duration_seconds_count{slice=\"inner_eval\"} 0";
        let summary = Summary::from_raw(
            &"prometheus_engine_query_duration_seconds",
            &raw_data.lines().collect(),
        );
        assert_eq!(summary.sum, "12".to_string());
        assert_eq!(
            summary.labels,
            Some(hashmap! {"slice".to_string() => "inner_eval".to_string()})
        );
    }

    #[test]
    fn histogram_parsing_works() {
        let raw_data = r#"prometheus_http_request_duration_seconds_bucket{handler="/metrics",le="0.1"} 10871
prometheus_http_request_duration_seconds_bucket{handler="/metrics",le="0.2"} 10871
prometheus_http_request_duration_seconds_bucket{handler="/metrics",le="0.4"} 10871
prometheus_http_request_duration_seconds_bucket{handler="/metrics",le="1"} 10871
prometheus_http_request_duration_seconds_bucket{handler="/metrics",le="3"} 10871
prometheus_http_request_duration_seconds_bucket{handler="/metrics",le="8"} 10871
prometheus_http_request_duration_seconds_bucket{handler="/metrics",le="20"} 10871
prometheus_http_request_duration_seconds_bucket{handler="/metrics",le="60"} 10871
prometheus_http_request_duration_seconds_bucket{handler="/metrics",le="120"} 10871
prometheus_http_request_duration_seconds_bucket{handler="/metrics",le="+Inf"} 10871
prometheus_http_request_duration_seconds_sum{handler="/metrics"} 67.48398663499978
prometheus_http_request_duration_seconds_count{handler="/metrics"} 10871"#;
        let histogram = Histogram::from_raw(
            &"prometheus_http_request_duration_seconds",
            &raw_data.lines().collect(),
        );
        assert_eq!(histogram.sum, "67.48398663499978");
        assert_eq!(
            histogram.labels,
            Some(hashmap! {"handler".to_string() => "/metrics".to_string()})
        );
    }
}
