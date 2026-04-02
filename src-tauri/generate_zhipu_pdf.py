#!/usr/bin/env python3
"""
智谱 (02513.HK) 股价分析PDF生成脚本
"""

from reportlab.lib.pagesizes import letter, A4
from reportlab.lib import colors
from reportlab.lib.styles import getSampleStyleSheet, ParagraphStyle
from reportlab.platypus import SimpleDocTemplate, Paragraph, Spacer, Table, TableStyle, PageBreak
from reportlab.lib.units import inch, cm
from reportlab.pdfgen import canvas
from datetime import datetime
import os

def create_zhipu_pdf():
    """创建智谱股价分析PDF"""
    
    # 创建输出文件名
    timestamp = datetime.now().strftime("%Y%m%d_%H%M%S")
    filename = f"智谱_02513HK_股价分析_{timestamp}.pdf"
    
    # 创建文档模板
    doc = SimpleDocTemplate(filename, pagesize=A4, 
                          rightMargin=72, leftMargin=72,
                          topMargin=72, bottomMargin=72)
    
    # 获取样式
    styles = getSampleStyleSheet()
    
    # 自定义样式
    title_style = ParagraphStyle(
        'CustomTitle',
        parent=styles['Heading1'],
        fontSize=24,
        textColor=colors.HexColor('#1E3A8A'),
        spaceAfter=30,
        alignment=1  # 居中
    )
    
    subtitle_style = ParagraphStyle(
        'CustomSubtitle',
        parent=styles['Heading2'],
        fontSize=16,
        textColor=colors.HexColor('#374151'),
        spaceAfter=20
    )
    
    section_style = ParagraphStyle(
        'CustomSection',
        parent=styles['Heading3'],
        fontSize=14,
        textColor=colors.HexColor('#047857'),
        spaceAfter=10
    )
    
    normal_style = ParagraphStyle(
        'CustomNormal',
        parent=styles['Normal'],
        fontSize=10,
        textColor=colors.HexColor('#1F2937'),
        spaceAfter=8
    )
    
    # 文档内容
    story = []
    
    # 标题页
    story.append(Paragraph("智谱AI (02513.HK) 股价分析报告", title_style))
    story.append(Spacer(1, 20))
    
    current_date = datetime.now().strftime("%Y年%m月%d日")
    story.append(Paragraph(f"报告日期: {current_date}", subtitle_style))
    story.append(Spacer(1, 40))
    
    # 公司简介
    story.append(Paragraph("一、公司简介", subtitle_style))
    story.append(Paragraph("智谱AI (GLM-4模型开发者) 是中国领先的人工智能公司，专注于大语言模型的研发和应用。", normal_style))
    story.append(Paragraph("• 股票代码: 02513.HK (港交所主板)", normal_style))
    story.append(Paragraph("• 交易货币: 港元 (HKD)", normal_style))
    story.append(Paragraph("• 所属行业: 人工智能/软件服务", normal_style))
    story.append(Paragraph("• 主要产品: GLM系列大模型、AI应用工具", normal_style))
    story.append(Spacer(1, 20))
    
    # 当前股价信息
    story.append(Paragraph("二、最新股价表现", subtitle_style))
    
    latest_price_data = [
        ["指标", "数值", "备注"],
        ["最新收盘价", "668.00 港元", "2026-03-27"],
        ["涨跌幅", "-2.34%", "较前日"],
        ["成交量", "168.71万股", "当日成交量"],
        ["成交额", "11.23亿港元", "当日成交额"],
        ["周涨跌幅", "+5.95%", "本周表现"],
        ["月涨跌幅", "+16.17%", "本月表现"],
        ["年涨跌幅", "+474.87%", "近一年表现"]
    ]
    
    price_table = Table(latest_price_data, colWidths=[3*cm, 4*cm, 5*cm])
    price_table.setStyle(TableStyle([
        ('BACKGROUND', (0, 0), (-1, 0), colors.HexColor('#3B82F6')),
        ('TEXTCOLOR', (0, 0), (-1, 0), colors.whitesmoke),
        ('ALIGN', (0, 0), (-1, -1), 'CENTER'),
        ('FONTNAME', (0, 0), (-1, 0), 'Helvetica-Bold'),
        ('FONTSIZE', (0, 0), (-1, 0), 11),
        ('BOTTOMPADDING', (0, 0), (-1, 0), 12),
        ('BACKGROUND', (0, 1), (-1, -1), colors.HexColor('#F3F4F6')),
        ('GRID', (0, 0), (-1, -1), 1, colors.black)
    ]))
    
    story.append(price_table)
    story.append(Spacer(1, 20))
    
    # 近期价格走势
    story.append(Paragraph("三、近期价格走势 (港元)", section_style))
    
    recent_prices = [
        ["日期", "收盘价", "日期", "收盘价"],
        ["2026-03-16", "605.0", "2026-03-22", "(缺数据)"],
        ["2026-03-17", "621.5", "2026-03-23", "590.0"],
        ["2026-03-18", "742.5", "2026-03-24", "655.0"],
        ["2026-03-19", "659.0", "2026-03-25", "760.0"],
        ["2026-03-20", "630.5", "2026-03-26", "684.0"]
    ]
    
    price_history_table = Table(recent_prices, colWidths=[3.5*cm, 3*cm, 3.5*cm, 3*cm])
    price_history_table.setStyle(TableStyle([
        ('BACKGROUND', (0, 0), (-1, 0), colors.HexColor('#10B981')),
        ('TEXTCOLOR', (0, 0), (-1, 0), colors.whitesmoke),
        ('ALIGN', (0, 0), (-1, -1), 'CENTER'),
        ('FONTNAME', (0, 0), (-1, 0), 'Helvetica-Bold'),
        ('GRID', (0, 0), (-1, -1), 1, colors.black),
        ('BACKGROUND', (0, 1), (-1, -1), colors.HexColor('#ECFDF5'))
    ]))
    
    story.append(price_history_table)
    story.append(Spacer(1, 20))
    
    # 分页
    story.append(PageBreak())
    
    # 估值指标
    story.append(Paragraph("四、估值指标分析", subtitle_style))
    
    valuation_data = [
        ["估值指标", "最新数值", "行业对比", "分析"],
        ["PE (TTM)", "-67.76倍", "行业中值: 25.11倍", "尚未盈利，PE为负"],
        ["PB (MRQ)", "-44.19倍", "行业中值: 4.01倍", "账面价值为负"],
        ["PS (TTM)", "841.91倍", "行业中值: 8.82倍", "超高市销率，反映高成长预期"],
        ["市值", "2,630亿港元", "AI公司中位列前茅", "与百度、腾讯等巨头相近"]
    ]
    
    valuation_table = Table(valuation_data, colWidths=[3*cm, 3.5*cm, 4*cm, 5*cm])
    valuation_table.setStyle(TableStyle([
        ('BACKGROUND', (0, 0), (-1, 0), colors.HexColor('#8B5CF6')),
        ('TEXTCOLOR', (0, 0), (-1, 0), colors.whitesmoke),
        ('ALIGN', (0, 0), (-1, -1), 'CENTER'),
        ('FONTNAME', (0, 0), (-1, 0), 'Helvetica-Bold'),
        ('GRID', (0, 0), (-1, -1), 1, colors.black),
        ('BACKGROUND', (0, 1), (-1, -1), colors.HexColor('#F5F3FF'))
    ]))
    
    story.append(valuation_table)
    story.append(Spacer(1, 20))
    
    # 可比公司分析
    story.append(Paragraph("五、可比公司估值对比", section_style))
    
    comp_companies = [
        ["排名", "公司", "代码", "市值(亿)", "PE", "PS"],
        ["1", "谷歌A", "GOOGL.O", "229,191", "25.11", "8.24"],
        ["2", "微软", "MSFT.O", "182,959", "26.02", "9.40"],
        ["3", "Meta", "META.O", "91,840", "22.00", "6.62"],
        ["4", "腾讯控股", "0700.HK", "39,795", "17.70", "5.29"],
        ["5", "阿里巴巴", "9988.HK", "20,680", "15.97", "2.08"],
        ["8", "智谱AI", "2513.HK", "2,630", "(未盈利)", "841.91"],
        ["中位值", "", "", "2,687", "25.11", "8.82"]
    ]
    
    comp_table = Table(comp_companies, colWidths=[1.5*cm, 4*cm, 2.5*cm, 3*cm, 2.5*cm, 3*cm])
    comp_table.setStyle(TableStyle([
        ('BACKGROUND', (0, 0), (-1, 0), colors.HexColor('#F59E0B')),
        ('TEXTCOLOR', (0, 0), (-1, 0), colors.whitesmoke),
        ('ALIGN', (0, 0), (-1, -1), 'CENTER'),
        ('FONTNAME', (0, 0), (-1, 0), 'Helvetica-Bold'),
        ('GRID', (0, 0), (-1, -1), 1, colors.black),
        ('BACKGROUND', (0, 1), (5, -1), colors.HexColor('#FFFBEB')),
        ('BACKGROUND', (6, 6), (6, 6), colors.HexColor('#FEF3C7')),
        ('TEXTCOLOR', (6, 6), (6, 6), colors.brown)
    ]))
    
    story.append(comp_table)
    story.append(Spacer(1, 20))
    
    # 分页
    story.append(PageBreak())
    
    # 财务表现
    story.append(Paragraph("六、财务业绩摘要", subtitle_style))
    
    financial_data = [
        ["报告期", "营业收入(万元)", "同比增长", "净利润(万元)", "每股收益(元)"],
        ["2025中报", "19,088", "+325.03%", "-235,117", "-62.27"],
        ["2024年报", "31,241", "+150.85%", "-295,649", "-87.20"],
        ["2024中报", "4,491", "+260.64%", "-123,555", "-36.67"],
        ["2023年报", "12,454", "(基准)", "-78,796", "-29.46"]
    ]
    
    financial_table = Table(financial_data, colWidths=[3*cm, 4*cm, 3*cm, 4*cm, 3*cm])
    financial_table.setStyle(TableStyle([
        ('BACKGROUND', (0, 0), (-1, 0), colors.HexColor('#EF4444')),
        ('TEXTCOLOR', (0, 0), (-1, 0), colors.whitesmoke),
        ('ALIGN', (0, 0), (-1, -1), 'CENTER'),
        ('FONTNAME', (0, 0), (-1, 0), 'Helvetica-Bold'),
        ('GRID', (0, 0), (-1, -1), 1, colors.black),
        ('BACKGROUND', (0, 1), (-1, -1), colors.HexColor('#FEE2E2'))
    ]))
    
    story.append(financial_table)
    story.append(Spacer(1, 20))
    
    # 投资要点
    story.append(Paragraph("七、投资要点分析", section_style))
    
    story.append(Paragraph("<b>优势：</b>", normal_style))
    story.append(Paragraph("• 中国AI大模型龙头企业，技术实力突出", normal_style))
    story.append(Paragraph("• 收入快速增长，显示市场认可度提升", normal_style))
    story.append(Paragraph("• 高市值反映市场对AI赛道的高预期", normal_style))
    story.append(Paragraph("• 港股通标的，流动性较好", normal_style))
    
    story.append(Spacer(1, 10))
    
    story.append(Paragraph("<b>风险：</b>", normal_style))
    story.append(Paragraph("• 尚未盈利，PE/PB均为负值", normal_style))
    story.append(Paragraph("• 市销率极高，估值可能存在泡沫", normal_style))
    story.append(Paragraph("• AI行业竞争激烈，技术迭代快", normal_style))
    story.append(Paragraph("• 监管政策变化可能影响业务", normal_style))
    
    story.append(Spacer(1, 10))
    
    story.append(Paragraph("<b>市场表现：</b>", normal_style))
    story.append(Paragraph("• 近一年涨幅474.87%，表现极为突出", normal_style))
    story.append(Paragraph("• 成交量活跃，市场关注度高", normal_style))
    story.append(Paragraph("• 波动较大，适合风险承受能力较强的投资者", normal_style))
    
    story.append(Spacer(1, 20))
    
    # 免责声明
    story.append(Paragraph("八、免责声明", subtitle_style))
    disclaimer = """本报告基于公开信息整理，仅供投资者参考。报告中的数据和信息来源于公开渠道，可能存在滞后或不完整的情况。
    
    本报告不构成任何投资建议，投资者应独立判断并承担投资风险。股市有风险，投资需谨慎。"""
    
    story.append(Paragraph(disclaimer, normal_style))
    story.append(Spacer(1, 20))
    
    # 生成PDF
    doc.build(story)
    
    print(f"PDF文件已生成: {filename}")
    print(f"文件大小: {os.path.getsize(filename)/1024:.1f} KB")
    print(f"保存路径: {os.path.abspath(filename)}")
    
    return filename

if __name__ == "__main__":
    try:
        pdf_file = create_zhipu_pdf()
        print("\n✅ PDF生成成功！")
        print(f"📄 文件: {pdf_file}")
    except Exception as e:
        print(f"❌ 生成PDF时出错: {e}")
        import traceback
        traceback.print_exc()