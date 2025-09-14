#!/bin/bash

# OpenAct Provider 测试 - 报告生成器

# 生成测试报告
generate_report() {
    local format="${1:-console}"
    
    log_info "生成测试报告 (格式: $format)..."
    
    case "$format" in
        "console")
            generate_console_report
            ;;
        "json")
            generate_json_report
            ;;
        "html")
            generate_html_report
            ;;
        *)
            log_error "不支持的报告格式: $format"
            return 1
            ;;
    esac
}

# 生成控制台报告
generate_console_report() {
    echo ""
    echo "📊 测试报告"
    echo "============"
    echo "Provider: $PROVIDER"
    echo "认证类型: $AUTH_TYPE"
    echo "测试时间: $(date)"
    echo ""
    
    # 测试阶段结果
    echo "测试阶段结果:"
    echo "-------------"
    
    local overall_status="success"
    
    for stage in configuration authentication actions integration; do
        local status="${TEST_RESULTS[$stage]:-unknown}"
        if [ "$status" = "failed" ]; then
            overall_status="failed"
        fi
        printf "%-15s: %s\n" "$stage" "$(format_status $status)"
    done
    
    echo ""
    
    # Action详细结果
    if [ ${#ACTION_RESULTS[@]} -gt 0 ]; then
        echo "Action测试结果:"
        echo "---------------"
        
        for action in "${!ACTION_RESULTS[@]}"; do
            local status="${ACTION_RESULTS[$action]}"
            printf "%-20s: %s\n" "$action" "$(format_status $status)"
        done
        
        echo ""
    fi
    
    # 总体结果
    echo "总体结果: $(format_status $overall_status)"
    
    if [ "$overall_status" = "success" ]; then
        echo ""
        echo "🎉 所有测试通过！"
    else
        echo ""
        echo "❌ 部分测试失败，请检查上面的详细信息。"
    fi
}

# 生成JSON报告
generate_json_report() {
    local report_file="$TEST_TEMP_DIR/test_report.json"
    
    # 构建Action结果JSON
    local actions_json="{"
    local first_action=true
    
    for action in "${!ACTION_RESULTS[@]}"; do
        if [ "$first_action" = true ]; then
            first_action=false
        else
            actions_json="$actions_json,"
        fi
        actions_json="$actions_json\"$action\":\"${ACTION_RESULTS[$action]}\""
    done
    actions_json="$actions_json}"
    
    # 生成完整报告
    cat > "$report_file" << EOF
{
    "test_info": {
        "provider": "$PROVIDER",
        "auth_type": "$AUTH_TYPE",
        "actions": "$ACTIONS",
        "tenant": "$TENANT",
        "test_id": "$TEST_ID",
        "timestamp": "$(get_timestamp)",
        "duration": $((${END_TIME:-$(date +%s)} - ${START_TIME:-$(date +%s)}))
    },
    "environment": {
        "authflow_url": "http://localhost:$AUTHFLOW_PORT",
        "provider_base_url": "$PROVIDER_BASE_URL",
        "connection_trn": "$CONNECTION_TRN"
    },
    "test_results": {
        "configuration": "${TEST_RESULTS[configuration]:-unknown}",
        "authentication": "${TEST_RESULTS[authentication]:-unknown}",
        "actions": "${TEST_RESULTS[actions]:-unknown}",
        "integration": "${TEST_RESULTS[integration]:-unknown}"
    },
    "action_results": $actions_json,
    "overall_status": "$(get_overall_status)"
}
EOF
    
    echo "JSON报告已生成: $report_file"
    
    if [ "$VERBOSE" = true ]; then
        echo ""
        echo "报告内容:"
        cat "$report_file" | jq '.'
    fi
}

# 生成HTML报告
generate_html_report() {
    local report_file="$TEST_TEMP_DIR/test_report.html"
    
    cat > "$report_file" << 'EOF'
<!DOCTYPE html>
<html lang="zh-CN">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>OpenAct Provider 测试报告</title>
    <style>
        body { font-family: Arial, sans-serif; margin: 20px; }
        .header { background: #f5f5f5; padding: 20px; border-radius: 5px; }
        .section { margin: 20px 0; }
        .success { color: #28a745; }
        .failed { color: #dc3545; }
        .unknown { color: #6c757d; }
        table { width: 100%; border-collapse: collapse; margin: 10px 0; }
        th, td { padding: 10px; text-align: left; border-bottom: 1px solid #ddd; }
        th { background-color: #f8f9fa; }
        .status-success { background-color: #d4edda; }
        .status-failed { background-color: #f8d7da; }
    </style>
</head>
<body>
    <div class="header">
        <h1>OpenAct Provider 测试报告</h1>
        <p><strong>Provider:</strong> PROVIDER_PLACEHOLDER</p>
        <p><strong>认证类型:</strong> AUTH_TYPE_PLACEHOLDER</p>
        <p><strong>测试时间:</strong> TIMESTAMP_PLACEHOLDER</p>
    </div>

    <div class="section">
        <h2>测试阶段结果</h2>
        <table>
            <thead>
                <tr><th>测试阶段</th><th>状态</th></tr>
            </thead>
            <tbody>
                STAGE_RESULTS_PLACEHOLDER
            </tbody>
        </table>
    </div>

    <div class="section">
        <h2>Action测试结果</h2>
        <table>
            <thead>
                <tr><th>Action</th><th>状态</th></tr>
            </thead>
            <tbody>
                ACTION_RESULTS_PLACEHOLDER
            </tbody>
        </table>
    </div>

    <div class="section">
        <h2>总体结果</h2>
        <p class="OVERALL_STATUS_CLASS">OVERALL_STATUS_PLACEHOLDER</p>
    </div>
</body>
</html>
EOF
    
    # 替换占位符
    sed -i.bak "s/PROVIDER_PLACEHOLDER/$PROVIDER/g" "$report_file"
    sed -i.bak "s/AUTH_TYPE_PLACEHOLDER/$AUTH_TYPE/g" "$report_file"
    sed -i.bak "s/TIMESTAMP_PLACEHOLDER/$(date)/g" "$report_file"
    
    # 生成阶段结果表格
    local stage_rows=""
    for stage in configuration authentication actions integration; do
        local status="${TEST_RESULTS[$stage]:-unknown}"
        local status_class="status-$status"
        stage_rows="$stage_rows<tr class=\"$status_class\"><td>$stage</td><td>$status</td></tr>"
    done
    sed -i.bak "s/STAGE_RESULTS_PLACEHOLDER/$stage_rows/g" "$report_file"
    
    # 生成Action结果表格
    local action_rows=""
    for action in "${!ACTION_RESULTS[@]}"; do
        local status="${ACTION_RESULTS[$action]}"
        local status_class="status-$status"
        action_rows="$action_rows<tr class=\"$status_class\"><td>$action</td><td>$status</td></tr>"
    done
    sed -i.bak "s/ACTION_RESULTS_PLACEHOLDER/$action_rows/g" "$report_file"
    
    # 设置总体状态
    local overall_status=$(get_overall_status)
    local overall_class="$overall_status"
    sed -i.bak "s/OVERALL_STATUS_PLACEHOLDER/$overall_status/g" "$report_file"
    sed -i.bak "s/OVERALL_STATUS_CLASS/$overall_class/g" "$report_file"
    
    # 清理备份文件
    rm -f "$report_file.bak"
    
    echo "HTML报告已生成: $report_file"
    
    if command -v open >/dev/null 2>&1; then
        echo "正在打开HTML报告..."
        open "$report_file"
    fi
}

# 获取总体状态
get_overall_status() {
    for stage in configuration authentication actions integration; do
        local status="${TEST_RESULTS[$stage]:-unknown}"
        if [ "$status" = "failed" ]; then
            echo "failed"
            return
        fi
    done
    echo "success"
}

# 保存报告到文件
save_report_to_file() {
    local format="$1"
    local output_file="$2"
    
    if [ -z "$output_file" ]; then
        output_file="test_report_${PROVIDER}_$(date +%Y%m%d_%H%M%S).$format"
    fi
    
    case "$format" in
        "json")
            generate_json_report
            cp "$TEST_TEMP_DIR/test_report.json" "$output_file"
            ;;
        "html")
            generate_html_report
            cp "$TEST_TEMP_DIR/test_report.html" "$output_file"
            ;;
        *)
            log_error "不支持保存格式: $format"
            return 1
            ;;
    esac
    
    log_success "报告已保存: $output_file"
}
